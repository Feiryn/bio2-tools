use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum CommandResponse {
    /// Response to Ping command
    /// The pong value must be the ping value + 1
    Pong { sequence_number: u8, pong_value: u8 },
    /// Response to Version command, contains version information about the device
    /// The name should be either BI2A or BI2X
    Version {
        sequence_number: u8,
        major: u8,
        minor: u8,
        patch: u8,
        name: String,
        date: String,
        time: String,
    },
    /// Response to StartFlash host command
    /// The ok field indicates whether the device successfully entered flash mode
    StartFlash { sequence_number: u8, ok: bool },
    /// After each command in flash mode where the device is waiting for the next block, it sends this response
    ReadyForNextFlashBlock,
    /// Response to AreYouInFlashMode flash command, indicates whether the device is in flash mode or not
    InFlashingModeOk,
    /// Response to AreYouInFlashMode flash command, indicates that the device is not in flash mode or there was an error
    InFlashingModeNotOk,
    /// Response to WriteBlock flash command, contains the offset in the firmware where the block has been written along with the block data
    /// The values are decoded here
    WriteBlockOk {
        firmware_offset: u16,
        block_data: [u8; 128],
    },
    /// Response to WriteBlock flash command, indicates that there was an error writing the block to the device
    WriteBlockNotOk,
    /// Response to StartWrite flash command
    /// The offset should be the same as the offset in the StartWrite command
    StartWriteOk { offset: u32 },
    /// Response to EndWrite flash command, indicates that the device successfully finished writing blocks
    EndWriteOk,
    /// Response to EndWrite flash command, indicates that there was an error finishing writing blocks
    EndWriteNotOk,
}

impl CommandResponse {
    fn from_aa_seq_command_and_payload(
        sequence_number: u8,
        command_code: u16,
        payload: &[u8],
    ) -> Result<Self, CommandResponseParseError> {
        match command_code {
            0x0001 => {
                if payload.len() != 1 {
                    return Err(CommandResponseParseError::InvalidPayloadLength {
                        expected: 1,
                        actual: payload.len(),
                    });
                }

                Ok(CommandResponse::Pong {
                    sequence_number,
                    pong_value: payload[0],
                })
            }
            0x0002 => {
                if payload.len() != 44 {
                    return Err(CommandResponseParseError::InvalidPayloadLength {
                        expected: 44,
                        actual: payload.len(),
                    });
                }

                Ok(CommandResponse::Version {
                    sequence_number,
                    major: payload[5],
                    minor: payload[6],
                    patch: payload[7],
                    name: String::from_utf8_lossy(&payload[8..12])
                        .trim_end_matches('\0')
                        .to_string(),
                    date: String::from_utf8_lossy(&payload[12..23])
                        .trim_end_matches('\0')
                        .to_string(),
                    time: String::from_utf8_lossy(&payload[28..37])
                        .trim_end_matches('\0')
                        .to_string(),
                })
            }
            0x0004 => {
                if payload.len() != 1 {
                    return Err(CommandResponseParseError::InvalidPayloadLength {
                        expected: 1,
                        actual: payload.len(),
                    });
                }

                Ok(CommandResponse::StartFlash {
                    sequence_number,
                    ok: payload[0] == 0x00,
                })
            }
            _ => Err(CommandResponseParseError::UnknownCommand {
                code: command_code,
                payload: payload.to_vec(),
            }),
        }
    }

    fn parse_aa_frame(frame: &[u8]) -> Result<Self, CommandResponseParseError> {
        // Verify that it is from the device (source byte should be 0x80)
        if frame.len() < 3 {
            return Err(CommandResponseParseError::MissingSourceByte);
        }
        /*
        if frame[2] != 0x80 {
            return Err(DeviceResponseParseError::InvalidSourceByte(frame[2]));
        }
        */
        // todo response must set the high bit to 1

        // Parse the command code
        if frame.len() < 5 {
            return Err(CommandResponseParseError::MissingCommandCode);
        }
        let command_code = ((frame[3] as u16) << 8) | (frame[4] as u16);

        // Parse the sequence number
        if frame.len() < 6 {
            return Err(CommandResponseParseError::MissingSequenceNumber);
        }
        let sequence_number = frame[5];

        // Parse the payload length
        if frame.len() < 7 {
            return Err(CommandResponseParseError::MissingPayloadLength);
        }
        let payload_length = frame[6];

        // Verify that the size of the frame matches the expected size based on the payload length + checksum
        let expected_length = 7 + payload_length as usize + 1; // 7 bytes before payload, payload length, 1 byte checksum
        if frame.len() != expected_length {
            return Err(CommandResponseParseError::InvalidFrameLength {
                expected: expected_length,
                actual: frame.len(),
            });
        }

        // Parse the payload
        let payload = &frame[7..7 + payload_length as usize];

        // Parse the checksum
        let checksum = frame[7 + payload_length as usize];

        // Verify the checksum (simple sum of all bytes modulo 256)
        let calculated_checksum: u8 = frame[2..7 + payload_length as usize]
            .iter()
            .fold(0u8, |acc, &byte| acc.wrapping_add(byte));
        if checksum != calculated_checksum {
            return Err(CommandResponseParseError::ChecksumMismatch {
                expected: calculated_checksum,
                actual: checksum,
            });
        }

        Self::from_aa_seq_command_and_payload(sequence_number, command_code, payload)
    }

    fn parse_a0_frame(frame: &[u8]) -> Result<Self, CommandResponseParseError> {
        if frame.len() != 1 {
            return Err(CommandResponseParseError::InvalidFrameLength {
                expected: 1,
                actual: frame.len(),
            });
        }

        Ok(CommandResponse::ReadyForNextFlashBlock)
    }

    fn parse_a1_frame(frame: &[u8]) -> Result<Self, CommandResponseParseError> {
        if frame.len() != 2 {
            return Err(CommandResponseParseError::InvalidFrameLength {
                expected: 2,
                actual: frame.len(),
            });
        }

        if frame[1] != 0xAC {
            Ok(CommandResponse::InFlashingModeNotOk)
        } else {
            Ok(CommandResponse::InFlashingModeOk)
        }
    }

    fn parse_a2_frame(frame: &[u8]) -> Result<Self, CommandResponseParseError> {
        if frame.len() != 135 {
            return Err(CommandResponseParseError::InvalidFrameLength {
                expected: 135,
                actual: frame.len(),
            });
        }

        // This is 0x4B because 0x4B ^ 0xB5 = 0xFE, which is the start of the firmware offset on the board
        if frame[1] != 0x4B {
            return Err(CommandResponseParseError::InvalidA2CommandByte1(frame[1]));
        }

        let hi = frame[2] ^ 0xFE;
        let lo = frame[3] ^ hi;
        let firmware_offset = ((hi as u16) << 8) | (lo as u16);

        let firmware_offset_checksum = frame[4];
        let calculated_checksum = (0x4b + frame[2] as u64 + frame[3] as u64) % 256;

        if firmware_offset_checksum != calculated_checksum as u8 {
            return Err(CommandResponseParseError::InvalidFirmwareOffsetChecksum {
                expected: firmware_offset_checksum,
                calculated: calculated_checksum as u8,
            });
        }

        let mut key_byte = lo ^ frame[4];
        let mut decoded_block = [0u8; 128];
        let mut checksum: u64 = 0;

        for i in 0..64 {
            let enc1 = frame[5 + i * 2] ^ key_byte;
            let enc2 = frame[6 + i * 2] ^ enc1;
            key_byte = enc2;
            decoded_block[i * 2] = enc1;
            decoded_block[i * 2 + 1] = enc2;
            checksum += frame[5 + i * 2] as u64 + frame[6 + i * 2] as u64;
        }

        let final_checksum = (checksum % 256) as u8;

        if frame[133] != final_checksum {
            return Err(
                CommandResponseParseError::InvalidFirmwareBlockDataChecksum {
                    expected: frame[133],
                    calculated: final_checksum,
                },
            );
        }

        if frame[134] != 0xAC {
            Ok(CommandResponse::WriteBlockNotOk)
        } else {
            Ok(CommandResponse::WriteBlockOk {
                firmware_offset,
                block_data: decoded_block,
            })
        }
    }

    fn parse_a8_frame(frame: &[u8]) -> Result<Self, CommandResponseParseError> {
        if frame.len() == 5 {
            Ok(CommandResponse::StartWriteOk {
                offset: ((frame[1] as u32) << 24)
                    | ((frame[2] as u32) << 16)
                    | ((frame[3] as u32) << 8)
                    | (frame[4] as u32),
            })
        } else if frame.len() == 2 {
            if frame[1] == 0xAC {
                Ok(CommandResponse::EndWriteOk)
            } else {
                Ok(CommandResponse::EndWriteNotOk)
            }
        } else {
            Err(CommandResponseParseError::InvalidFrameLength {
                expected: 5,
                actual: frame.len(),
            })
        }
    }

    pub fn from_frame(frame: &[u8], unescape: bool) -> Result<Self, CommandResponseParseError> {
        if frame.len() == 0 {
            return Err(CommandResponseParseError::EmptyFrame);
        }

        let escaped_frame = if unescape {
            let mut unescaped = Vec::new();
            let mut i = if frame[0] == 0xAA { 2 } else { 1 };

            for j in 0..i {
                if j >= frame.len() {
                    return Err(CommandResponseParseError::InvalidFrameLength {
                        expected: i,
                        actual: frame.len(),
                    });
                }
                unescaped.push(frame[j]);
            }

            while i < frame.len() {
                if frame[i] == 0xFF {
                    if i + 1 >= frame.len() {
                        return Err(CommandResponseParseError::UnexpectedEndOfFrameAfterFF);
                    }
                    unescaped.push(!frame[i + 1]);
                    i += 1;
                } else {
                    unescaped.push(frame[i]);
                }
                i += 1;
            }

            unescaped
        } else {
            frame.to_vec()
        };

        if escaped_frame[0] == 0xAA {
            CommandResponse::parse_aa_frame(&escaped_frame)
        } else if escaped_frame[0] == 0xA0 {
            CommandResponse::parse_a0_frame(&escaped_frame)
        } else if escaped_frame[0] == 0xA1 {
            CommandResponse::parse_a1_frame(&escaped_frame)
        } else if escaped_frame[0] == 0xA2 {
            CommandResponse::parse_a2_frame(&escaped_frame)
        } else if escaped_frame[0] == 0xA8 {
            CommandResponse::parse_a8_frame(&escaped_frame)
        } else {
            Err(CommandResponseParseError::UnknownStartByte(
                escaped_frame[0],
            ))
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommandResponseParseError {
    #[error("Empty frame")]
    EmptyFrame,
    #[error("Escaping error: unexpected end of frame after 0xFF")]
    UnexpectedEndOfFrameAfterFF,
    #[error("Missing source byte")]
    MissingSourceByte,
    #[error("Invalid source byte: expected 0x80, got {0:#04x}")]
    InvalidSourceByte(u8),
    #[error("Missing command code")]
    MissingCommandCode,
    #[error("Missing sequence number")]
    MissingSequenceNumber,
    #[error("Missing payload length")]
    MissingPayloadLength,
    #[error("Invalid frame length: expected {expected} bytes, got {actual} bytes")]
    InvalidFrameLength { expected: usize, actual: usize },
    #[error("Checksum mismatch: expected {expected:#04x}, is instead {actual:#04x}")]
    ChecksumMismatch { expected: u8, actual: u8 },
    #[error("A2 command byte 1 should be 0x4B, got {0:#04x}")]
    InvalidA2CommandByte1(u8),
    #[error(
        "Invalid firmware offset checksum: expected {expected:#04x}, calculated {calculated:#04x}"
    )]
    InvalidFirmwareOffsetChecksum { expected: u8, calculated: u8 },
    #[error(
        "Invalid firmware block data checksum: expected {expected:#04x}, calculated {calculated:#04x}"
    )]
    InvalidFirmwareBlockDataChecksum { expected: u8, calculated: u8 },
    #[error("Invalid payload length: expected {expected} bytes, got {actual} bytes")]
    InvalidPayloadLength { expected: usize, actual: usize },
    #[error("Unknown command code: {code:#06x} with payload: {payload:?}")]
    UnknownCommand { code: u16, payload: Vec<u8> },
    #[error("Unknown start byte: {0:#04x}")]
    UnknownStartByte(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pong_response() {
        let frame = vec![0xAA, 0xAA, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0x03];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::Pong {
                sequence_number,
                pong_value,
            } => {
                assert_eq!(sequence_number, 0x00);
                assert_eq!(pong_value, 0x01);
            }
            _ => panic!("Expected Pong response"),
        }
    }

    #[test]
    fn test_parse_version_response() {
        let frame = vec![
            0xAA, 0xAA, 0x80, 0x00, 0x02, 0x01, 0x2C, 0x0D, 0x06, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x0E, 0x42, 0x49, 0x32, 0x41, 0x4F, 0x63, 0x74, 0x20, 0x33, 0x31, 0x20, 0x32, 0x30,
            0x31, 0x38, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x39, 0x3A, 0x31, 0x31, 0x3A, 0x35,
            0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0E,
        ];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::Version {
                sequence_number,
                major,
                minor,
                patch,
                name,
                date,
                time: hour,
            } => {
                assert_eq!(sequence_number, 0x01);
                assert_eq!(major, 0x01);
                assert_eq!(minor, 0x02);
                assert_eq!(patch, 0x0E);
                assert_eq!(name, "BI2A");
                assert_eq!(date, "Oct 31 2018");
                assert_eq!(hour, "19:11:53");
            }
            _ => panic!("Expected Version response"),
        }
    }

    #[test]
    fn test_parse_start_flash_response() {
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x12, 0x01, 0x00, 0x97];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::StartFlash {
                sequence_number,
                ok,
            } => {
                assert_eq!(sequence_number, 0x12);
                assert_eq!(ok, true);
            }
            _ => panic!("Expected StartFlash response"),
        }

        let frame_not_ok = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x12, 0x01, 0x01, 0x98];
        let response_not_ok = CommandResponse::from_frame(&frame_not_ok, false).unwrap();
        match response_not_ok {
            CommandResponse::StartFlash {
                sequence_number,
                ok,
            } => {
                assert_eq!(sequence_number, 0x12);
                assert_eq!(ok, false);
            }
            _ => panic!("Expected StartFlash response"),
        }
    }

    #[test]
    fn parse_ready_for_next_flash_block_response() {
        let frame = vec![0xA0];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::ReadyForNextFlashBlock => {}
            _ => panic!("Expected ReadyForNextFlashBlock response"),
        }
    }

    #[test]
    fn parse_in_flashing_mode_response() {
        let frame_ok = vec![0xA1, 0xAC];
        let response_ok = CommandResponse::from_frame(&frame_ok, false).unwrap();
        match response_ok {
            CommandResponse::InFlashingModeOk => {}
            _ => panic!("Expected InFlashingModeOk response"),
        }

        let frame_not_ok = vec![0xA1, 0xAF];
        let response_not_ok = CommandResponse::from_frame(&frame_not_ok, false).unwrap();
        match response_not_ok {
            CommandResponse::InFlashingModeNotOk => {}
            _ => panic!("Expected InFlashingModeNotOk response"),
        }
    }

    #[test]
    fn parse_flashed_block_ok_response() {
        let frame = vec![
            0xA2, 0x4B, 0x01, 0x7F, 0xCB, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x39, 0x78, 0x0D, 0x09, 0x6B,
            0x7B, 0x06, 0x7D, 0x73, 0x5D, 0x54, 0x1C, 0x01, 0x15, 0x97, 0x13, 0x27, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xFF, 0x01, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xFB,
            0x05, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xF7, 0x09, 0xFE, 0x00, 0xEF, 0x11, 0xFE,
            0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xF3,
            0x0D, 0xFE, 0x00, 0xFF, 0xF0, 0x0F, 0x00, 0xF2, 0xAC,
        ];
        let expected_block_data = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0x00, 0x00, 0x00, 0x00, 0x39, 0x41, 0x4C, 0x45, 0x2E, 0x55, 0x53, 0x2E, 0x5D, 0x00,
            0x54, 0x48, 0x49, 0x5C, 0xCB, 0xD8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x01, 0xFF, 0xFF,
            0x10, 0x01, 0xFF, 0xFF, 0x10, 0x01, 0xFF, 0xFF, 0x04, 0x01, 0xFF, 0xFF, 0x10, 0x01,
            0xFF, 0xFF, 0x08, 0x01, 0xFF, 0xFF, 0x10, 0x01, 0xFF, 0xFF, 0x10, 0x01, 0xFF, 0xFF,
            0x10, 0x01, 0xFF, 0xFF, 0x10, 0x01, 0xFF, 0xFF, 0x0C, 0x01, 0xFF, 0xFF, 0x00, 0xF0,
            0xFF, 0xFF,
        ];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::WriteBlockOk {
                firmware_offset,
                block_data,
            } => {
                assert_eq!(firmware_offset, 0xFF80);
                assert_eq!(block_data, expected_block_data);
            }
            _ => panic!("Expected FlashedBlockOk response"),
        }

        let frame_not_ok = vec![
            0xA2, 0x4B, 0x01, 0x7F, 0xCB, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x39, 0x78, 0x0D, 0x09, 0x6B,
            0x7B, 0x06, 0x7D, 0x73, 0x5D, 0x54, 0x1C, 0x01, 0x15, 0x97, 0x13, 0x27, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xFF, 0x01, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xFB,
            0x05, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xF7, 0x09, 0xFE, 0x00, 0xEF, 0x11, 0xFE,
            0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xEF, 0x11, 0xFE, 0x00, 0xF3,
            0x0D, 0xFE, 0x00, 0xFF, 0xF0, 0x0F, 0x00, 0xF2, 0xAF,
        ];
        let response_not_ok = CommandResponse::from_frame(&frame_not_ok, false).unwrap();
        match response_not_ok {
            CommandResponse::WriteBlockNotOk => {}
            _ => panic!("Expected FlashedBlockNotOk response"),
        }
    }

    #[test]
    fn parse_start_write_block_ok_response() {
        let frame = vec![0xA8, 0x00, 0x00, 0x00, 0x00];
        let response = CommandResponse::from_frame(&frame, false).unwrap();
        match response {
            CommandResponse::StartWriteOk { offset } => {
                assert_eq!(offset, 0x00000000);
            }
            _ => panic!("Expected StartWriteBlockOk response"),
        }
    }

    #[test]
    fn parse_end_write_block_ok_response() {
        let frame_ok = vec![0xA8, 0xAC];
        let response_ok = CommandResponse::from_frame(&frame_ok, false).unwrap();
        match response_ok {
            CommandResponse::EndWriteOk => {}
            _ => panic!("Expected EndWriteBlockOk response"),
        }

        let frame_not_ok = vec![0xA8, 0xAF];
        let response_not_ok = CommandResponse::from_frame(&frame_not_ok, false).unwrap();
        match response_not_ok {
            CommandResponse::EndWriteNotOk => {}
            _ => panic!("Expected EndWriteBlockNotOk response"),
        }
    }

    // --- Error tests ---

    #[test]
    fn test_error_empty_frame() {
        let err = CommandResponse::from_frame(&[], false).unwrap_err();
        assert!(matches!(err, CommandResponseParseError::EmptyFrame));
    }

    #[test]
    fn test_error_unexpected_end_of_frame_after_ff() {
        // escape=true: frame[0] is the outer marker, frame[1..] is unescaped.
        // 0xFF at the last position means no byte follows it.
        let frame = vec![0xA0, 0xFF];
        let err = CommandResponse::from_frame(&frame, true).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::UnexpectedEndOfFrameAfterFF
        ));
    }

    #[test]
    fn test_error_unknown_start_byte() {
        let frame = vec![0xB0];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::UnknownStartByte(0xB0)
        ));
    }

    // AA frame errors

    #[test]
    fn test_error_aa_missing_source_byte() {
        // AA frame with only 2 bytes: [0xAA, 0xAA] → len < 3
        let frame = vec![0xAA, 0xAA];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(err, CommandResponseParseError::MissingSourceByte));
    }

    #[test]
    fn test_error_aa_missing_command_code() {
        // len=4 → len < 5
        let frame = vec![0xAA, 0xAA, 0x80, 0x00];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(err, CommandResponseParseError::MissingCommandCode));
    }

    #[test]
    fn test_error_aa_missing_sequence_number() {
        // len=5 → len < 6
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x01];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::MissingSequenceNumber
        ));
    }

    #[test]
    fn test_error_aa_missing_payload_length() {
        // len=6 → len < 7
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x01, 0x00];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::MissingPayloadLength
        ));
    }

    #[test]
    fn test_error_aa_invalid_frame_length() {
        // payload_length=1 → expected_length = 7+1+1 = 9, but frame has 10 bytes
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x01, 0x00, 0x01, 0x01, 0x03, 0x00];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFrameLength {
                expected: 9,
                actual: 10
            }
        ));
    }

    #[test]
    fn test_error_aa_checksum_mismatch() {
        // Pong frame with correct length but wrong checksum byte (0xFF instead of 0x03)
        // Calculated over frame[2..8]: 0x00+0x00+0x01+0x00+0x01+0x01 = 0x03
        let frame = vec![0xAA, 0xAA, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0xFF];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::ChecksumMismatch {
                expected: 0x03,
                actual: 0xFF
            }
        ));
    }

    #[test]
    fn test_error_aa_unknown_command() {
        // Command code 0x0099, payload 1 byte (0x00)
        // checksum over frame[2..8]: 0x80+0x00+0x99+0x00+0x01+0x00 = 282 → 0x1A
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x99, 0x00, 0x01, 0x00, 0x1A];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        match err {
            CommandResponseParseError::UnknownCommand { code, payload } => {
                assert_eq!(code, 0x0099);
                assert_eq!(payload, vec![0x00]);
            }
            _ => panic!("Expected UnknownCommand error"),
        }
    }

    #[test]
    fn test_error_aa_invalid_payload_length_pong() {
        // Pong (0x0001) with 2-byte payload instead of 1
        // checksum over frame[2..9]: 0x80+0x00+0x01+0x00+0x02+0x01+0x02 = 134 = 0x86
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x01, 0x00, 0x02, 0x01, 0x02, 0x86];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidPayloadLength {
                expected: 1,
                actual: 2
            }
        ));
    }

    #[test]
    fn test_error_aa_invalid_payload_length_version() {
        // Version (0x0002) with 1-byte payload instead of 44
        // checksum over frame[2..8]: 0x80+0x00+0x02+0x00+0x01+0x00 = 131 = 0x83
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x02, 0x00, 0x01, 0x00, 0x83];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidPayloadLength {
                expected: 44,
                actual: 1
            }
        ));
    }

    #[test]
    fn test_error_aa_invalid_payload_length_start_flash() {
        // StartFlash (0x0004) with 2-byte payload instead of 1
        // checksum over frame[2..9]: 0x80+0x00+0x04+0x00+0x02+0x00+0x00 = 134 = 0x86
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x00, 0x02, 0x00, 0x00, 0x86];
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidPayloadLength {
                expected: 1,
                actual: 2
            }
        ));
    }

    // A0 frame errors

    #[test]
    fn test_error_a0_invalid_frame_length() {
        let frame = vec![0xA0, 0x00]; // len=2, expected=1
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFrameLength {
                expected: 1,
                actual: 2
            }
        ));
    }

    // A1 frame errors

    #[test]
    fn test_error_a1_invalid_frame_length() {
        let frame = vec![0xA1]; // len=1, expected=2
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFrameLength {
                expected: 2,
                actual: 1
            }
        ));
    }

    // A2 frame errors

    #[test]
    fn test_error_a2_invalid_frame_length() {
        let frame = vec![0xA2; 10]; // len=10, expected=135
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFrameLength {
                expected: 135,
                actual: 10
            }
        ));
    }

    #[test]
    fn test_error_a2_invalid_command_byte1() {
        let mut frame = vec![0u8; 135];
        frame[0] = 0xA2;
        frame[1] = 0x00; // must be 0x4B
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidA2CommandByte1(0x00)
        ));
    }

    #[test]
    fn test_error_a2_invalid_firmware_offset_checksum() {
        let mut frame = vec![0u8; 135];
        frame[0] = 0xA2;
        frame[1] = 0x4B;
        frame[2] = 0x01;
        frame[3] = 0x7F;
        frame[4] = 0x00; // wrong: (0x4B + 0x01 + 0x7F) % 256 = 0xCB
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFirmwareOffsetChecksum {
                expected: 0x00,
                calculated: 0xCB
            }
        ));
    }

    #[test]
    fn test_error_a2_invalid_firmware_block_data_checksum() {
        let mut frame = vec![0u8; 135];
        frame[0] = 0xA2;
        frame[1] = 0x4B;
        frame[2] = 0x01;
        frame[3] = 0x7F;
        frame[4] = 0xCB; // correct: (0x4B + 0x01 + 0x7F) % 256 = 0xCB
        // frame[5..133] are all 0x00, so block data checksum = 0x00
        frame[133] = 0xFF; // wrong, should be 0x00
        frame[134] = 0xAC;
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFirmwareBlockDataChecksum {
                expected: 0xFF,
                calculated: 0x00
            }
        ));
    }

    // A8 frame errors

    #[test]
    fn test_error_a8_invalid_frame_length() {
        let frame = vec![0xA8, 0x00, 0x00]; // len=3, not 2 or 5
        let err = CommandResponse::from_frame(&frame, false).unwrap_err();
        assert!(matches!(
            err,
            CommandResponseParseError::InvalidFrameLength {
                expected: 5,
                actual: 3
            }
        ));
    }
}
