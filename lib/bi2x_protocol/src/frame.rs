use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum Bi2xFrameMode {
    Compressed,       // Flags bits = (1, 0, 0). Uses MZ compression
    Raw,              // Flags bits = (0, 1, 0). Nothing is done to the payload
    Escaped,          // Flags bits = (0, 0, 0). 0xFF bytes escape the next byte by inverting it
    ByteSubstitution, // Flags bits = (0, 1, 1). 0xAA are replaced by the first byte of the payload
}

#[derive(Debug, PartialEq, Eq)]
pub struct Bi2xFrame {
    pub node: u8,
    pub sequence_number: u8,
    pub mode: Bi2xFrameMode,
    pub is_encrypted: bool,
    pub payload: Option<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum Bi2xFrameParseError {
    #[error("Frame is empty")]
    EmptyFrame,
    #[error("Invalid start marker: expected 0xAA, got {0:#02X}")]
    InvalidStartMarker(u8),
    #[error("Invalid frame length: expected {expected}, got {actual}")]
    InvalidFrameLength { expected: usize, actual: usize },
    #[error("VLQ length is too long: more than 4 continuation bytes")]
    LengthTooLong,
    #[error("VLQ length is invalid: contains invalid byte")]
    InvalidLengthByte,
    #[error("VLQ length is truncated: no terminal byte found")]
    LengthTruncated,
    #[error("VLQ length calculation overflowed")]
    LengthOverflow,
    #[error("Invalid frame mode: {0:#02X}")]
    InvalidFrameMode(u8),
    #[error("Header CRC mismatch: expected {expected:#02X}, got {actual:#02X}")]
    HeaderCrcMismatch { expected: u8, actual: u8 },
    #[error("Payload CRC mismatch: expected {expected:#02X}, got {actual:#02X}")]
    PayloadCrcMismatch { expected: u8, actual: u8 },
    #[error("Payload shouldn't be empty at this point. This is likely a bug in the parser.")]
    EmptyPayloadAtWrongTime,
}

#[derive(Debug, Error)]
pub enum Bi2xFrameToBytesError {
    #[error("VLQ length encoding overflowed")]
    LengthOverflow,
    #[error("Failed to find a suitable substitution byte for ByteSubstitution mode")]
    NoSuitableSubstitutionByte,
}

impl Bi2xFrame {
    /// Decodes a VLQ-encoded length from the start of `data`.
    ///
    /// Byte classification (matching frame_parser state 3):
    ///   0x00–0x7F : terminal     (7 data bits, bit 7 clear)
    ///   0xC0–0xFF : continuation (6 data bits, bits 7+6 both set)
    ///   0x80–0xBF : invalid      (bit 7 set, bit 6 clear)
    ///
    /// Returns `Some((length, bytes_consumed))` or `None` if the input is
    /// truncated, invalid, or would overflow u32.
    fn decode_length(data: &[u8]) -> Result<(u32, usize), Bi2xFrameParseError> {
        let mut val: u32 = 0;

        for (i, &b) in data.iter().enumerate() {
            if (b & 0xC0) == 0xC0 {
                // Continuation byte — bits 7+6 both set, 6 data bits
                if i >= 4 {
                    return Err(Bi2xFrameParseError::LengthTooLong); // max 4 continuation bytes (byte_count > 4 -> error)
                }
                val = val
                    .checked_shl(6)
                    .ok_or_else(|| Bi2xFrameParseError::LengthOverflow)?
                    .checked_add((b & 0x3f) as u32)
                    .ok_or_else(|| Bi2xFrameParseError::LengthOverflow)?;
            } else if b & 0x80 == 0 {
                // Terminal byte — bit 7 clear, 7 data bits
                val = val
                    .checked_shl(7)
                    .ok_or_else(|| Bi2xFrameParseError::LengthOverflow)?
                    .checked_add((b & 0x7f) as u32)
                    .ok_or_else(|| Bi2xFrameParseError::LengthOverflow)?;
                return Ok((val, i + 1));
            } else {
                // Invalid byte — bit 7 set, bit 6 clear (0x80–0xBF)
                return Err(Bi2xFrameParseError::InvalidLengthByte);
            }
        }

        // Reached end of slice without a terminal byte
        Err(Bi2xFrameParseError::LengthTruncated)
    }

    fn encode_length(length: u32) -> Result<Vec<u8>, Bi2xFrameToBytesError> {
        if length <= 0x7F {
            Ok(vec![length as u8])
        } else if length <= 0x3FFF {
            Ok(vec![
                ((length >> 7) as u8 & 0x3F) | 0xC0,
                (length as u8 & 0x7F),
            ])
        } else if length <= 0x1FFFFF {
            Ok(vec![
                ((length >> 13) as u8 & 0x3F) | 0xC0,
                ((length >> 7) as u8 & 0x3F) | 0xC0,
                (length as u8 & 0x7F),
            ])
        } else if length <= 0xFFFFFFF {
            Ok(vec![
                ((length >> 19) as u8 & 0x3F) | 0xC0,
                ((length >> 13) as u8 & 0x3F) | 0xC0,
                ((length >> 7) as u8 & 0x3F) | 0xC0,
                (length as u8 & 0x7F),
            ])
        } else {
            Err(Bi2xFrameToBytesError::LengthOverflow)
        }
    }

    fn crc4_make_lgp_c(seed: u8, data: &[u8]) -> u8 {
        const CRC4_LGP_TABLE: [u8; 16] = [
            0x00, 0x0D, 0x03, 0x0E, 0x06, 0x0B, 0x05, 0x08, 0x0C, 0x01, 0x0F, 0x02, 0x0A, 0x07,
            0x09, 0x04,
        ];

        let mut crc = seed & 0xF;
        for &b in data {
            let tmp = CRC4_LGP_TABLE[usize::from((crc ^ b) & 0xF)];
            crc = CRC4_LGP_TABLE[usize::from((b >> 4 ^ tmp ^ crc >> 4) & 0xF)] ^ (tmp >> 4);
        }
        crc
    }

    fn get_header_crc(
        node: u8,
        sequence_number: u8,
        payload_bytes_count: u8,
        payload_length: u32,
        flags: u8,
    ) -> u8 {
        let mut crc = Self::crc4_make_lgp_c(0xF, &[node]);
        crc = Self::crc4_make_lgp_c(crc, &[sequence_number]);
        if payload_bytes_count >= 5 {
            crc = Self::crc4_make_lgp_c(crc, &[(((payload_length >> 25) as u8) & 0x3F) | 0xC0]);
        }
        if payload_bytes_count >= 4 {
            crc = Self::crc4_make_lgp_c(crc, &[(((payload_length >> 19) as u8) & 0x3F) | 0xC0]);
        }
        if payload_bytes_count >= 3 {
            crc = Self::crc4_make_lgp_c(crc, &[(((payload_length >> 13) as u8) & 0x3F) | 0xC0]);
        }
        if payload_bytes_count >= 2 {
            crc = Self::crc4_make_lgp_c(crc, &[(((payload_length >> 7) as u8) & 0x3F) | 0xC0]);
        }
        crc = Self::crc4_make_lgp_c(crc, &[payload_length as u8 & 0x7F]);
        crc = Self::crc4_make_lgp_c(crc, &[flags & 0xF0]);
        crc ^ 0xF
    }

    fn payload_crc(payload: &[u8]) -> u8 {
        pub const CRC7_LGP48_TABLE: [u8; 16] = [
            0x00, 0x09, 0x12, 0x1B, 0x24, 0x2D, 0x36, 0x3F, 0x48, 0x41, 0x5A, 0x53, 0x6C, 0x65,
            0x7E, 0x77,
        ];

        let mut crc = 0x7F;
        for &b in payload {
            let tmp = CRC7_LGP48_TABLE[usize::from((crc ^ b) & 0xF)] ^ (crc >> 4);
            crc = CRC7_LGP48_TABLE[usize::from(((b >> 4) ^ tmp) & 0xF)] ^ (tmp >> 4);
        }
        crc ^ 0x7F
    }

    /// Parses a Bi2xFrame from bytes.
    ///
    /// The `is_from_bio2` parameter should be set to true if the frame is from the board to the computer, and false otherwise, as the encryption is slightly different in these two cases.
    pub fn parse(bytes: &[u8], is_from_bio2: bool) -> Result<Self, Bi2xFrameParseError> {
        if bytes.len() == 0 {
            return Err(Bi2xFrameParseError::EmptyFrame);
        }

        let start_marker = bytes[0];
        if start_marker != 0xAA {
            return Err(Bi2xFrameParseError::InvalidStartMarker(start_marker));
        }

        // Remove all starting 0xAA bytes
        let mut bytes = bytes
            .iter()
            .skip_while(|&&b| b == 0xAA)
            .cloned()
            .collect::<Vec<u8>>();

        if bytes.len() < 3 {
            return Err(Bi2xFrameParseError::InvalidFrameLength {
                expected: 3,
                actual: bytes.len(),
            });
        }

        // Parse header
        let node = bytes[0];
        let sequence_number = bytes[1];
        let (length, mut bytes_consumed) = Self::decode_length(&bytes[2..])?;
        let payload_length_bytes = bytes[2..2 + bytes_consumed as usize].to_owned();

        if bytes.len() < 3 + bytes_consumed as usize {
            return Err(Bi2xFrameParseError::InvalidFrameLength {
                expected: 3 + bytes_consumed as usize,
                actual: bytes.len(),
            });
        }

        // Header CRC can be escaped
        if bytes[2 + bytes_consumed as usize] == 0xFF {
            bytes[3 + bytes_consumed as usize] = !bytes[2 + bytes_consumed as usize];
            bytes_consumed += 1;
        }

        // Parse flags
        let flag_byte_1 = bytes[2 + bytes_consumed as usize] & 0x80;
        let flag_byte_2 = bytes[2 + bytes_consumed as usize] & 0x40;
        let flag_byte_3 = bytes[2 + bytes_consumed as usize] & 0x20;
        let mode = match (flag_byte_1, flag_byte_2, flag_byte_3) {
            (0x80, 0x00, 0x00) => Ok(Bi2xFrameMode::Compressed),
            (0x00, 0x40, 0x00) => Ok(Bi2xFrameMode::Raw),
            (0x00, 0x00, 0x00) => Ok(Bi2xFrameMode::Escaped),
            (0x00, 0x40, 0x20) => Ok(Bi2xFrameMode::ByteSubstitution),
            (a, b, c) => Err(Bi2xFrameParseError::InvalidFrameMode(a | b | c)),
        }?;
        let is_encrypted = bytes[2 + bytes_consumed as usize] & 0x10 != 0;

        // Verify header CRC
        let header_crc = bytes[2 + bytes_consumed as usize] & 0x0F;
        let expected_header_crc = Self::get_header_crc(
            node,
            sequence_number,
            payload_length_bytes.len() as u8,
            length,
            bytes[2 + bytes_consumed as usize],
        );

        if header_crc != expected_header_crc {
            return Err(Bi2xFrameParseError::HeaderCrcMismatch {
                expected: expected_header_crc,
                actual: header_crc,
            });
        }

        let payload = if length != 0 {
            let mut payload = bytes[3 + bytes_consumed as usize..bytes.len()].to_vec();

            // Decrypt if needed
            if is_encrypted {
                payload = crypt::decrypt_bi2x_payload(sequence_number, &payload, is_from_bio2);
            }

            // Pop and store the payload CRC before any processing
            let payload_crc = payload
                .pop()
                .ok_or(Bi2xFrameParseError::EmptyPayloadAtWrongTime)?;

            match mode {
                Bi2xFrameMode::Escaped => {
                    // Bytes with value 0xFF are escaped by inverting the next byte.
                    let mut new_payload = Vec::with_capacity(payload.len());
                    let mut escape_next = false;
                    for &b in payload.iter() {
                        if escape_next {
                            new_payload.push(!b);
                            escape_next = false;
                        } else if b == 0xFF {
                            escape_next = true;
                        } else {
                            new_payload.push(b);
                        }
                    }

                    payload = new_payload;
                }
                Bi2xFrameMode::Compressed => {
                    // Use MC LZ decompression
                    payload = compression::mc_lz::inflate(&payload, length as usize);
                }
                Bi2xFrameMode::ByteSubstitution => {
                    // The first byte of the payload is the substitution byte. All occurrences of this byte in the payload are replaced by 0xAA, and then the substitution byte is removed from the payload.
                    let substitution_byte = payload[0];
                    for b in payload.iter_mut() {
                        if *b == substitution_byte {
                            *b = 0xAA;
                        }
                    }
                    payload = payload[1..].to_vec(); // Remove the substitution byte from the payload
                }
                Bi2xFrameMode::Raw => {}
            }

            // Verify payload CRC
            let computed_payload_crc = Self::payload_crc(&payload);
            if payload_crc != computed_payload_crc {
                return Err(Bi2xFrameParseError::PayloadCrcMismatch {
                    expected: computed_payload_crc,
                    actual: payload_crc,
                });
            }

            // Verify payload length
            if payload.len() != length as usize {
                return Err(Bi2xFrameParseError::InvalidFrameLength {
                    expected: length as usize,
                    actual: bytes.len(),
                });
            }

            Ok(Some(payload))
        } else {
            if bytes.len() != 3 + bytes_consumed as usize {
                return Err(Bi2xFrameParseError::InvalidFrameLength {
                    expected: 3 + bytes_consumed as usize,
                    actual: bytes.len(),
                });
            }

            Ok(None)
        }?;

        Ok(Self {
            node,
            sequence_number,
            mode,
            is_encrypted,
            payload,
        })
    }

    /// Returns the frame bytes.
    ///
    /// The `is_for_bio2` parameter should be set to true if the frame is from the computer to the baord, and false otherwise, as the encryption is slightly different in these two cases.
    pub fn to_bytes(&self, is_for_bio2: bool) -> Result<Vec<u8>, Bi2xFrameToBytesError> {
        let mut bytes = Vec::new();

        // Start marker and header
        bytes.push(0xAA);
        bytes.push(self.node);
        bytes.push(self.sequence_number);

        let payload_length = self.payload.as_ref().map_or(0, |p| p.len() as u32);
        let payload_length_bytes = Self::encode_length(payload_length)?;
        bytes.extend_from_slice(&payload_length_bytes);

        // Flags
        let flags = match self.mode {
            Bi2xFrameMode::Compressed => 0x80,
            Bi2xFrameMode::Raw => 0x40,
            Bi2xFrameMode::Escaped => 0x00,
            Bi2xFrameMode::ByteSubstitution => 0x60,
        } | if self.is_encrypted { 0x10 } else { 0x00 };

        // Calculate header CRC
        let header_crc = Self::get_header_crc(
            self.node,
            self.sequence_number,
            payload_length_bytes.len() as u8,
            payload_length,
            flags,
        );

        bytes.push(flags | header_crc);

        if let Some(payload) = &self.payload {
            let payload_crc = Self::payload_crc(payload);
            let mut payload_bytes = payload.clone();

            match self.mode {
                Bi2xFrameMode::Escaped => {
                    // Bytes with value 0xFF or 0xAA are escaped by inserting a 0xFF byte and inverting the byte.
                    let mut escaped_payload = Vec::with_capacity(payload_bytes.len());
                    for &b in payload_bytes.iter() {
                        if b == 0xFF || b == 0xAA {
                            escaped_payload.push(0xFF);
                            escaped_payload.push(!b);
                        } else {
                            escaped_payload.push(b);
                        }
                    }
                    payload_bytes = escaped_payload;
                }
                Bi2xFrameMode::Compressed => {
                    // Use MC LZ compression
                    payload_bytes = compression::mc_lz::deflate(payload_bytes.as_slice());
                }
                Bi2xFrameMode::ByteSubstitution => {
                    // Chose a substitution byte that is not present in the payload and is not 0xAA, replace all occurrences of 0xAA in the payload by this byte, and insert this byte at the start of the payload.
                    let mut substitution_byte = 0x01;
                    while payload_bytes.contains(&substitution_byte) || substitution_byte == 0xAA {
                        substitution_byte = substitution_byte.wrapping_add(1);
                        if substitution_byte == 0x00 {
                            return Err(Bi2xFrameToBytesError::NoSuitableSubstitutionByte);
                        }
                    }

                    for b in payload_bytes.iter_mut() {
                        if *b == 0xAA {
                            *b = substitution_byte;
                        }
                    }

                    payload_bytes.insert(0, substitution_byte);
                }
                Bi2xFrameMode::Raw => {}
            }

            payload_bytes.push(payload_crc);

            // Encrypt if needed
            if self.is_encrypted {
                payload_bytes =
                    crypt::encrypt_bi2x_payload(self.sequence_number, &payload_bytes, is_for_bio2);
            }

            bytes.extend_from_slice(&payload_bytes);
        }

        Ok(bytes)
    }

    /// Create a Bi2xFrame.
    /// The mode is determined based on the payload content and length:
    pub fn new(
        node: u8,
        sequence_number: u8,
        is_encrypted: bool,
        payload: Option<Vec<u8>>,
    ) -> Self {
        // Determine the mode based on the payload content and length
        let mode = if let Some(payload) = &payload {
            if payload.contains(&0xAA) {
                Bi2xFrameMode::ByteSubstitution
            } else if payload.len() > 7 {
                Bi2xFrameMode::Compressed
            } else {
                Bi2xFrameMode::Raw
            }
        } else {
            Bi2xFrameMode::Escaped
        };

        Self {
            node,
            sequence_number,
            mode,
            is_encrypted,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let frame = Bi2xFrame::new(0x01, 0x9D, false, None);
        assert_eq!(frame.mode, Bi2xFrameMode::Escaped);

        let frame = Bi2xFrame::new(0x01, 0x9D, false, Some(vec![0x00, 0x01]));
        assert_eq!(frame.mode, Bi2xFrameMode::Raw);

        let frame = Bi2xFrame::new(0x01, 0x9D, false, Some(vec![0x00; 10]));
        assert_eq!(frame.mode, Bi2xFrameMode::Compressed);

        let frame = Bi2xFrame::new(0x01, 0x9D, false, Some(vec![0x00, 0xAA, 0x01]));
        assert_eq!(frame.mode, Bi2xFrameMode::ByteSubstitution);
    }

    #[test]
    fn test_no_payload() {
        let frame = vec![0xaa, 0x00, 0x9d, 0x00, 0x0e];
        let expected_result = Bi2xFrame {
            node: 0x00,
            sequence_number: 0x9d,
            mode: Bi2xFrameMode::Escaped,
            is_encrypted: false,
            payload: None,
        };

        let parsed = Bi2xFrame::parse(&frame, false).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(true).unwrap();
        assert_eq!(serialized, frame);
    }

    #[test]
    fn test_raw() {
        let frame = vec![0xAA, 0x00, 0x9F, 0x02, 0x4e, 0x00, 0x01, 0x4a];
        let expected_result = Bi2xFrame {
            node: 0x00,
            sequence_number: 0x9F,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x01]),
        };

        let parsed = Bi2xFrame::parse(&frame, true).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(true).unwrap();
        assert_eq!(serialized, frame);
    }

    #[test]
    fn test_raw_and_encrypted() {
        let frame = vec![
            0xaa, 0x03, 0xa0, 0x23, 0x58, 0x7b, 0x1a, 0x71, 0x5b, 0x51, 0x44, 0x2c, 0x62, 0x72,
            0x32, 0x20, 0x6c, 0x06, 0x6e, 0x3d, 0x3b, 0xd7, 0x76, 0x85, 0x06, 0x47, 0x75, 0x16,
            0x12, 0x63, 0x60, 0x19, 0x5e, 0x3f, 0x0c, 0x55, 0xbb, 0x75, 0xdf, 0xb2, 0x14,
        ];
        let expected_result = Bi2xFrame {
            node: 0x03,
            sequence_number: 0xa0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: true,
            payload: Some(vec![
                0x00, 0x02, 0x00, 0x0d, 0x06, 0x00, 0x01, 0x00, 0x01, 0x02, 0x09, 0x42, 0x49, 0x32,
                0x58, 0x01, 0x96, 0x3e, 0xc4, 0x00, 0x00, 0x01, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0xbb, 0x1f, 0x8e, 0xe2,
            ]),
        };

        let parsed = Bi2xFrame::parse(&frame, true).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(false).unwrap();
        assert_eq!(serialized, frame);
    }

    #[test]
    fn test_compressed_and_encrypted() {
        let frame = vec![
            0xaa, 0x02, 0xa2, 0x47, 0x93, 0x6c, 0x4d, 0x11, 0x11, 0xa8, 0xb9, 0x7c, 0x3e, 0x2c,
            0xa1, 0x46, 0xff, 0x5b, 0x4c, 0xf1, 0x0b, 0x37, 0xf1, 0x60, 0x73, 0xba, 0x03, 0xff,
            0x08, 0x2a, 0x2e, 0x7f, 0x25, 0x66, 0x0b, 0x7b, 0x12, 0xf6, 0x56, 0x5c, 0x57, 0x2e,
            0x62, 0x76, 0x23, 0x3c, 0x2e, 0x4f, 0x5a, 0x76, 0x2d, 0x6b, 0x5c, 0x6a, 0x04, 0x65,
            0x25, 0x16, 0x31, 0x63, 0x78, 0x19, 0x52, 0x1b, 0x0c, 0x42, 0x66, 0x7b,
        ];
        let expected_result = Bi2xFrame {
            node: 0x02,
            sequence_number: 0xa2,
            mode: Bi2xFrameMode::Compressed,
            is_encrypted: true,
            payload: Some(vec![
                0x00, 0x13, 0x02, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x32, 0x00, 0x00, 0xa4, 0x1c, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x2d, 0x00, 0x00, 0xe5, 0x1c, 0x00, 0x00, 0xf0,
                0x2d, 0x00, 0x00, 0x00, 0xff, 0x08, 0x13, 0x00, 0x00, 0x09, 0x13, 0x01, 0x00, 0x0a,
                0x13, 0x02, 0x00, 0x0b, 0x13, 0x03, 0x00, 0x05, 0x13, 0x15, 0x00, 0x06, 0x13, 0x17,
                0x00, 0x14, 0x0b, 0x22, 0x00, 0x13, 0x0b, 0x23, 0x00, 0x18, 0x0c, 0x24, 0x00, 0x17,
                0x0c,
            ]),
        };

        let parsed = Bi2xFrame::parse(&frame, false).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(true).unwrap();
        assert_eq!(serialized, frame);
    }

    #[test]
    fn test_substition_byte_and_encrypted() {
        let frame = vec![
            0xaa, 0x02, 0xd0, 0x47, 0x79, 0x5b, 0x0b, 0x7b, 0x03, 0x26, 0x67, 0x1f, 0x7d, 0x25,
            0xa9, 0xff, 0x7f, 0x2e, 0x50, 0x3e, 0x4c, 0x64, 0x91, 0x1a, 0xee, 0xd1, 0x75, 0x56,
            0xb9, 0x56, 0xad, 0x63, 0x0c, 0x22, 0xed, 0x95, 0x73, 0x44, 0x25, 0x66, 0x0a, 0x3e,
            0x70, 0xec, 0x44, 0x3f, 0x94, 0xed, 0x17, 0x47, 0x19, 0xf5, 0x58, 0xa4, 0xbc, 0xf1,
            0x3a, 0x1a, 0x40, 0x95, 0x56, 0x0a, 0x3b, 0xe8, 0xeb, 0xf8, 0x72, 0x4e, 0x87, 0xf9,
            0x3f, 0x37, 0x32, 0x60, 0x14, 0x59, 0x66, 0x4b,
        ];
        let expected_result = Bi2xFrame {
            node: 0x02,
            sequence_number: 0xd0,
            mode: Bi2xFrameMode::ByteSubstitution,
            is_encrypted: true,
            payload: Some(vec![
                0x00, 0x13, 0x02, 0x00, 0x00, 0x0b, 0x40, 0x17, 0xa8, 0xff, 0x7f, 0x17, 0x2e, 0x61,
                0x60, 0x11, 0x91, 0x61, 0xee, 0xc1, 0x04, 0x00, 0xec, 0x12, 0xa8, 0xaa, 0x7f, 0x12,
                0xec, 0x91, 0x3c, 0x18, 0x40, 0x5c, 0x61, 0x76, 0x11, 0xe8, 0x03, 0x4b, 0x81, 0xfd,
                0x74, 0x27, 0x00, 0xa1, 0x67, 0xa0, 0xe9, 0xb1, 0x61, 0x62, 0x11, 0x81, 0x61, 0x2e,
                0x36, 0xa8, 0xeb, 0xa9, 0x62, 0x47, 0x83, 0xfc, 0x03, 0x72, 0x28, 0x2b, 0x3c, 0x18,
                0x00,
            ]),
        };

        let parsed = Bi2xFrame::parse(&frame, false).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(true).unwrap();
        assert_eq!(serialized, frame);
    }

    #[test]
    fn test_escaped_and_encrypted() {
        let frame = vec![
            0xaa, 0x02, 0x73, 0xc2, 0x02, 0x1c, 0x64, 0x34, 0x3d, 0x33, 0x01, 0x03, 0x3d, 0x7b,
            0x59, 0x2b, 0x7d, 0x03, 0x71, 0x13, 0x7d, 0x5b, 0x59, 0x4b, 0x3d, 0x73, 0x61, 0x23,
            0x3d, 0x3b, 0x59, 0x4b, 0x7d, 0x23, 0x71, 0x53, 0x7d, 0x1b, 0x59, 0x6b, 0x3d, 0x33,
            0x41, 0x43, 0x3d, 0x7b, 0x19, 0x2b, 0x7d, 0x43, 0x71, 0x53, 0x7d, 0x1b, 0x19, 0x0b,
            0x3d, 0x73, 0x61, 0x23, 0x3d, 0x3b, 0x19, 0x0b, 0x7d, 0x23, 0x71, 0x13, 0x7d, 0x5b,
            0x19, 0x6b, 0x3d, 0x33, 0x01, 0x03, 0x3d, 0x7b, 0x59, 0x2b, 0x7d, 0x03, 0x71, 0x13,
            0x7d, 0x5b, 0x59, 0x4b, 0x3d, 0x73, 0x61, 0x23, 0x3d, 0x3b, 0x59, 0x4b, 0x7d, 0x23,
            0x71, 0x53, 0x7d, 0x1b, 0x59, 0x6b, 0x3d, 0x33, 0x41, 0x43, 0x3d, 0x7b, 0x19, 0x2b,
            0x7d, 0x43, 0x71, 0x53, 0x7d, 0x1b, 0x19, 0x0b, 0x3d, 0x73, 0x61, 0x23, 0x3d, 0x3b,
            0x19, 0x0b, 0x7d, 0x23, 0x71, 0x13, 0x7d, 0x5b, 0x19, 0x6b, 0x95, 0x91, 0x83, 0x83,
            0x95, 0xd1, 0xd3, 0x83, 0xdd, 0x89, 0xdb, 0x9b, 0xdd, 0xd9, 0xdb, 0xcb, 0x95, 0xd1,
            0xc3, 0x83, 0x95, 0x91, 0xd3, 0xc3, 0xdd, 0x89, 0xdb, 0xdb, 0xdd, 0x99, 0xdb, 0xcb,
            0xb5, 0xb1, 0xe3, 0xe3, 0xb5, 0xf1, 0xb3, 0xa3, 0xfd, 0xe9, 0xff, 0x0e, 0xab, 0xfc,
            0xfc, 0xae, 0xaf, 0xa4, 0xa4, 0xb6, 0xb6, 0xf4, 0xe4, 0xa6, 0xb6, 0xbc, 0xbc, 0xba,
            0xbb, 0xa8, 0xf8, 0xbe, 0xbf, 0xd0, 0x80, 0xc2, 0x82, 0x80, 0xc0, 0x92, 0x92, 0x98,
            0x88, 0x8a, 0x9a, 0xd8, 0xd8, 0x8a, 0xda, 0x90, 0xc0, 0x82, 0xc2, 0xc0, 0xc0, 0xd2,
            0x92, 0xd8, 0xc8, 0x8a, 0x9a, 0x98, 0xd8, 0xca, 0xda, 0xb0, 0xe0, 0xe2, 0xe2, 0xa0,
            0xe0, 0xf2, 0xb2, 0xb8, 0xe8, 0xea, 0xeb, 0xec, 0xbc, 0xee, 0xef, 0xe4, 0xa4, 0xb6,
            0xe6, 0xf4, 0xe4, 0xe6, 0xe6, 0xac, 0xac, 0xfa, 0xfb, 0xb8, 0xb8, 0xfe, 0xff, 0x02,
            0x42,
        ];
        let expected_result = Bi2xFrame {
            node: 0x02,
            sequence_number: 0x73,
            mode: Bi2xFrameMode::Escaped,
            is_encrypted: true,
            payload: Some(vec![
                0x03, 0x20, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
                0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
                0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
                0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35,
                0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43,
                0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51,
                0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f,
                0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d,
                0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b,
                0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
                0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
                0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5,
                0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb0, 0xb1, 0xb2, 0xb3,
                0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0, 0xc1,
                0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf,
                0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xdb, 0xdc, 0xdd,
                0xde, 0xdf, 0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xeb,
                0xec, 0xed, 0xee, 0xef, 0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9,
                0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
            ]),
        };

        let parsed = Bi2xFrame::parse(&frame, false).unwrap();
        assert_eq!(parsed, expected_result);

        let serialized = parsed.to_bytes(true).unwrap();
        assert_eq!(serialized, frame);
    }

    // --- Error path tests ---

    #[test]
    fn test_empty_frame() {
        let result = Bi2xFrame::parse(&[], false);
        assert!(matches!(result, Err(Bi2xFrameParseError::EmptyFrame)));
    }

    #[test]
    fn test_invalid_start_marker() {
        let frame = vec![0x00, 0x00, 0x9d, 0x00, 0x0e];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidStartMarker(0x00))
        ));
    }

    #[test]
    fn test_frame_too_short_after_start() {
        // After stripping the leading 0xAA only 2 bytes remain; parser needs at least 3.
        let frame = vec![0xAA, 0x00, 0x9d];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidFrameLength {
                expected: 3,
                actual: 2
            })
        ));
    }

    #[test]
    fn test_invalid_length_byte() {
        // 0x80 sits in the 0x80–0xBF range: bit 7 set, bit 6 clear → invalid VLQ byte.
        let frame = vec![0xAA, 0x00, 0x9d, 0x80];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidLengthByte)
        ));
    }

    #[test]
    fn test_length_truncated() {
        // 0xC0 is a VLQ continuation byte; the slice ends without a terminal byte.
        let frame = vec![0xAA, 0x00, 0x9d, 0xC0];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(result, Err(Bi2xFrameParseError::LengthTruncated)));
    }

    #[test]
    fn test_length_too_long() {
        // Five consecutive continuation bytes (0xC0) exceed the 4-byte limit.
        let frame = vec![0xAA, 0x00, 0x9d, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(result, Err(Bi2xFrameParseError::LengthTooLong)));
    }

    #[test]
    fn test_frame_too_short_for_flags() {
        // VLQ declares length=2 but the frame is cut off before the flags/CRC byte.
        let frame = vec![0xAA, 0x00, 0x9F, 0x02];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidFrameLength {
                expected: 4,
                actual: 3
            })
        ));
    }

    #[test]
    fn test_header_crc_mismatch() {
        // Identical to test_no_payload but the CRC nibble is 0xF instead of 0xE.
        let frame = vec![0xaa, 0x00, 0x9d, 0x00, 0x0f];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::HeaderCrcMismatch {
                expected: 0xE,
                actual: 0xF
            })
        ));
    }

    #[test]
    fn test_invalid_frame_mode() {
        // Flags byte 0xA1: mode bits 7+5 both set (0xA0) are not a valid combination.
        // CRC nibble 0x1 is correct for (node=0x00, seq=0x9d, length=0, flags_high=0xA0).
        let frame = vec![0xAA, 0x00, 0x9d, 0x00, 0xA1];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidFrameMode(0xA0))
        ));
    }

    #[test]
    fn test_payload_crc_mismatch() {
        // Raw frame from test_raw with first payload byte flipped (0x00 → 0x01).
        let frame = vec![0xAA, 0x00, 0x9F, 0x02, 0x4e, 0x01, 0x01, 0x4a];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::PayloadCrcMismatch { .. })
        ));
    }

    #[test]
    fn test_empty_payload_at_wrong_time() {
        // Header declares length=1 (Raw mode) but no payload bytes are present.
        // Flags byte 0x43: Raw mode (0x40) + CRC nibble 0x3
        // (verified for node=0x00, seq=0x9F, length=1).
        let frame = vec![0xAA, 0x00, 0x9F, 0x01, 0x43];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::EmptyPayloadAtWrongTime)
        ));
    }

    #[test]
    fn test_zero_length_with_trailing_bytes() {
        // Valid no-payload frame (length=0) with an unexpected trailing byte.
        let frame = vec![0xaa, 0x00, 0x9d, 0x00, 0x0e, 0x00];
        let result = Bi2xFrame::parse(&frame, false);
        assert!(matches!(
            result,
            Err(Bi2xFrameParseError::InvalidFrameLength {
                expected: 4,
                actual: 5
            })
        ));
    }
}
