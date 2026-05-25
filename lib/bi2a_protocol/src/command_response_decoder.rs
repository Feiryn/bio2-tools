use thiserror::Error;

use crate::command_response::{CommandResponse, CommandResponseParseError};

/// Decoder for command responses received over the serial port. The decoder maintains
/// internal state to accumulate bytes until a complete frame is received, at which point it parses
/// the frame into a CommandResponse and resets its state for the next frame. The decoder also
/// handles escaping of 0xFF bytes
pub struct CommandResponseDecoder {
    bytes: Vec<u8>,
    expected_length: Option<usize>,
    escaping: bool,
}

impl CommandResponseDecoder {
    pub fn new() -> Self {
        CommandResponseDecoder {
            bytes: Vec::new(),
            expected_length: None,
            escaping: false,
        }
    }

    /// Update the decoder with a new byte. Returns Ok(Some(CommandResponse)) if a complete frame is
    /// received and successfully parsed, Ok(None) if the byte was accepted but a complete frame is not
    /// yet received, or Err(...) if the byte is invalid in the current state (e.g. unexpected start byte)
    pub fn update(
        &mut self,
        byte: u8,
    ) -> Result<Option<CommandResponse>, CommandResponseDecoderError> {
        if self.bytes.len() == 0 {
            if byte == 0xA0 {
                self.expected_length = Some(1);
            } else if byte & 0xF0 != 0xA0 {
                return Err(CommandResponseDecoderError::UnknownStartByte(byte));
            }
            self.bytes.push(byte);
        } else if self.bytes.len() == 1 && self.bytes[0] == 0xA8 {
            if byte & 0xF0 == 0xA0 {
                self.expected_length = Some(2);
            } else {
                self.expected_length = Some(5);
            }
            if byte == 0xFF {
                self.escaping = true;
                return Ok(None);
            }
            self.bytes.push(byte);
        } else if self.escaping {
            self.escaping = false;
            self.bytes.push(!byte);
        } else if byte == 0xFF {
            self.escaping = true;
            return Ok(None);
        } else {
            if self.bytes[0] != 0xAA && byte & 0xF0 == 0xA0 {
                self.expected_length = Some(self.bytes.len() + 1);
            }
            self.bytes.push(byte);
        }

        if self.bytes[0] == 0xAA {
            if self.bytes.len() == 7 {
                self.expected_length = Some((self.bytes[6] as usize) + 8); // 7 bytes of header + payload length + checksum
            }
        }

        if let Some(expected_length) = self.expected_length {
            if self.bytes.len() == expected_length {
                let frame = self.bytes.to_owned();
                self.reset();
                return Ok(Some(CommandResponse::from_frame(frame.as_slice(), false)?));
            }
        }

        return Ok(None);
    }

    pub fn update_get_raw(
        &mut self,
        byte: u8,
    ) -> Result<Option<(CommandResponse, Vec<u8>)>, CommandResponseDecoderError> {
        if self.bytes.len() == 0 {
            if byte == 0xA0 {
                self.expected_length = Some(1);
            } else if byte & 0xF0 != 0xA0 {
                return Err(CommandResponseDecoderError::UnknownStartByte(byte));
            }
            self.bytes.push(byte);
        } else if self.bytes.len() == 1 && self.bytes[0] == 0xA8 {
            if byte & 0xF0 == 0xA0 {
                self.expected_length = Some(2);
            } else {
                self.expected_length = Some(5);
            }
            if byte == 0xFF {
                self.escaping = true;
                return Ok(None);
            }
            self.bytes.push(byte);
        } else if self.escaping {
            self.escaping = false;
            self.bytes.push(!byte);
        } else if byte == 0xFF {
            self.escaping = true;
            return Ok(None);
        } else {
            if self.bytes[0] != 0xAA && byte & 0xF0 == 0xA0 {
                self.expected_length = Some(self.bytes.len() + 1);
            }
            self.bytes.push(byte);
        }

        if self.bytes[0] == 0xAA {
            if self.bytes.len() == 7 {
                self.expected_length = Some((self.bytes[6] as usize) + 8); // 7 bytes of header + payload length + checksum
            }
        }

        if let Some(expected_length) = self.expected_length {
            if self.bytes.len() == expected_length {
                let frame = self.bytes.to_owned();
                self.reset();
                return Ok(Some((
                    CommandResponse::from_frame(frame.as_slice(), false)?,
                    frame,
                )));
            }
        }

        return Ok(None);
    }

    pub fn reset(&mut self) {
        self.bytes.clear();
        self.expected_length = None;
        self.escaping = false;
    }
}

#[derive(Debug, Error)]
pub enum CommandResponseDecoderError {
    #[error("Unknown start byte: {0:#04x}")]
    UnknownStartByte(u8),
    #[error("Failed to parse host command response: {0}")]
    CommandResponseParseError(#[from] CommandResponseParseError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a raw frame for the decoder by escaping 0xFF bytes as [0xFF, 0x00].
    fn escape_frame(raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &b in raw {
            if b == 0xFF {
                out.push(0xFF);
                out.push(0x00);
            } else {
                out.push(b);
            }
        }
        out
    }

    fn assert_valid_response(frame: Vec<u8>, expected_response: CommandResponse) {
        let mut decoder = CommandResponseDecoder::new();
        for i in 0..frame.len() {
            let response = decoder.update(frame[i]).unwrap();
            if response.is_some() {
                assert!(i == frame.len() - 1);
                assert_eq!(response, Some(expected_response));
                break;
            } else {
                assert!(i < frame.len() - 1);
            }
        }
    }

    #[test]
    fn test_aa_ping() {
        let frame = vec![0xAA, 0xAA, 0x00, 0x00, 0x01, 0x53, 0x01, 0x05, 0x5A];
        assert_valid_response(
            frame,
            CommandResponse::Pong {
                sequence_number: 0x53,
                pong_value: 0x05,
            },
        );
    }

    #[test]
    fn test_aa_version() {
        let frame = vec![
            0xAA, 0xAA, 0x80, 0x00, 0x02, 0x01, 0x2C, 0x0D, 0x06, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x0E, 0x42, 0x49, 0x32, 0x41, 0x4F, 0x63, 0x74, 0x20, 0x33, 0x31, 0x20, 0x32, 0x30,
            0x31, 0x38, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x39, 0x3A, 0x31, 0x31, 0x3A, 0x35,
            0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0E,
        ];
        assert_valid_response(
            frame,
            CommandResponse::Version {
                sequence_number: 0x01,
                major: 0x01,
                minor: 0x02,
                patch: 0x0E,
                name: "BI2A".to_string(),
                date: "Oct 31 2018".to_string(),
                time: "19:11:53".to_string(),
            },
        );
    }

    #[test]
    fn test_aa_start_flash() {
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x12, 0x01, 0x00, 0x97];
        assert_valid_response(
            frame,
            CommandResponse::StartFlash {
                sequence_number: 0x12,
                ok: true,
            },
        );

        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x12, 0x01, 0x01, 0x98];
        assert_valid_response(
            frame,
            CommandResponse::StartFlash {
                sequence_number: 0x12,
                ok: false,
            },
        );
    }

    #[test]
    fn test_a0_ready_for_next_flash_block() {
        let frame = vec![0xA0];
        assert_valid_response(frame, CommandResponse::ReadyForNextFlashBlock);
    }

    #[test]
    fn test_a1_in_flashing_mode() {
        let frame_ok = vec![0xA1, 0xAC];
        assert_valid_response(frame_ok, CommandResponse::InFlashingModeOk);

        let frame_not_ok = vec![0xA1, 0xAF];
        assert_valid_response(frame_not_ok, CommandResponse::InFlashingModeNotOk);
    }

    #[test]
    fn test_a2_flashed_block_ok() {
        // Raw frame as passed to from_frame(..., false); escape 0xFF bytes for the decoder wire format.
        let raw_frame = vec![
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
        let frame = escape_frame(&raw_frame);
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
        assert_valid_response(
            frame,
            CommandResponse::WriteBlockOk {
                firmware_offset: 0xFF80,
                block_data: expected_block_data,
            },
        );

        let raw_frame_not_ok = vec![
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
        assert_valid_response(
            escape_frame(&raw_frame_not_ok),
            CommandResponse::WriteBlockNotOk,
        );
    }

    #[test]
    fn test_a8_start_write_ok() {
        let frame = vec![0xA8, 0x00, 0x00, 0x00, 0xA0];
        assert_valid_response(frame, CommandResponse::StartWriteOk { offset: 0x000000A0 });

        // Non-zero offset: [0xA8, hi, b2, b3, 0xAx-terminator]
        // Use 0xA8 as terminator (0xA8 & 0xF0 == 0xA0), offset = 0x01020308
        let frame2 = vec![0xA8, 0x01, 0x02, 0x03, 0xA8];
        assert_valid_response(frame2, CommandResponse::StartWriteOk { offset: 0x010203A8 });

        let frame = vec![0xA8, 0x00, 0x00, 0x00, 0x00];
        assert_valid_response(frame, CommandResponse::StartWriteOk { offset: 0x00000000 });
    }

    #[test]
    fn test_a8_end_write_block() {
        let frame_ok = vec![0xA8, 0xAC];
        assert_valid_response(frame_ok, CommandResponse::EndWriteOk);

        let frame_not_ok = vec![0xA8, 0xAF];
        assert_valid_response(frame_not_ok, CommandResponse::EndWriteNotOk);
    }

    // --- Error tests ---

    fn assert_error(frame: Vec<u8>, check: impl Fn(&CommandResponseDecoderError)) {
        let mut decoder = CommandResponseDecoder::new();
        let mut found_err = false;
        for &byte in &frame {
            match decoder.update(byte) {
                Err(e) => {
                    check(&e);
                    found_err = true;
                    break;
                }
                Ok(_) => {}
            }
        }
        assert!(found_err, "Expected an error but none was produced");
    }

    #[test]
    fn test_error_unknown_start_byte() {
        assert_error(vec![0xB0], |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::UnknownStartByte(0xB0)
            ));
        });
    }

    #[test]
    fn test_error_aa_checksum_mismatch() {
        // Pong frame with wrong checksum byte (0xFE instead of 0x03)
        // Using 0xFE to avoid the decoder's 0xFF escape handling
        let frame = vec![0xAA, 0xAA, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0xFE];
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::ChecksumMismatch {
                        expected: 0x03,
                        actual: 0xFE
                    }
                )
            ));
        });
    }

    #[test]
    fn test_error_aa_unknown_command() {
        // Command code 0x0099, payload 1 byte (0x00)
        // checksum over frame[2..8]: 0x80+0x00+0x99+0x00+0x01+0x00 = 0x1A
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x99, 0x00, 0x01, 0x00, 0x1A];
        assert_error(frame, |e| match e {
            CommandResponseDecoderError::CommandResponseParseError(
                CommandResponseParseError::UnknownCommand { code, payload },
            ) => {
                assert_eq!(*code, 0x0099);
                assert_eq!(*payload, vec![0x00]);
            }
            _ => panic!("Expected UnknownCommand error"),
        });
    }

    #[test]
    fn test_error_aa_invalid_payload_length_pong() {
        // Pong (0x0001) with 2-byte payload instead of 1
        // checksum over frame[2..9]: 0x80+0x00+0x01+0x00+0x02+0x01+0x02 = 0x86
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x01, 0x00, 0x02, 0x01, 0x02, 0x86];
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidPayloadLength {
                        expected: 1,
                        actual: 2
                    }
                )
            ));
        });
    }

    #[test]
    fn test_error_aa_invalid_payload_length_version() {
        // Version (0x0002) with 1-byte payload instead of 44
        // checksum over frame[2..8]: 0x80+0x00+0x02+0x00+0x01+0x00 = 0x83
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x02, 0x00, 0x01, 0x00, 0x83];
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidPayloadLength {
                        expected: 44,
                        actual: 1
                    }
                )
            ));
        });
    }

    #[test]
    fn test_error_aa_invalid_payload_length_start_flash() {
        // StartFlash (0x0004) with 2-byte payload instead of 1
        // checksum over frame[2..9]: 0x80+0x00+0x04+0x00+0x02+0x00+0x00 = 0x86
        let frame = vec![0xAA, 0xAA, 0x80, 0x00, 0x04, 0x00, 0x02, 0x00, 0x00, 0x86];
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidPayloadLength {
                        expected: 1,
                        actual: 2
                    }
                )
            ));
        });
    }

    #[test]
    fn test_error_a2_invalid_command_byte1() {
        // A2 frame with byte[1] != 0x4B; last byte must be 0xAx to terminate the frame.
        let mut frame = vec![0u8; 135];
        frame[0] = 0xA2;
        frame[1] = 0x00; // must be 0x4B
        frame[134] = 0xAC; // terminator
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidA2CommandByte1(0x00)
                )
            ));
        });
    }

    #[test]
    fn test_error_a2_invalid_firmware_offset_checksum() {
        let mut frame = vec![0u8; 135];
        frame[0] = 0xA2;
        frame[1] = 0x4B;
        frame[2] = 0x01;
        frame[3] = 0x7F;
        frame[4] = 0x00; // wrong: (0x4B + 0x01 + 0x7F) % 256 = 0xCB
        frame[134] = 0xAC; // terminator
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidFirmwareOffsetChecksum {
                        expected: 0x00,
                        calculated: 0xCB
                    }
                )
            ));
        });
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
        // Use 0xFE (not 0xFF) to avoid the decoder's 0xFF escape handling
        frame[133] = 0xFE; // wrong expected checksum
        frame[134] = 0xAC; // terminator
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidFirmwareBlockDataChecksum {
                        expected: 0xFE,
                        calculated: 0x00
                    }
                )
            ));
        });
    }

    #[test]
    fn test_error_a8_invalid_frame_length() {
        // 3-byte A8 frame: [0xA8, 0x00, 0xA1] — third byte (0xA1) is 0xAx so it terminates
        // from_frame sees [0xA8, 0x00, 0xA1], len=3, expects 2 or 5 → InvalidFrameLength { expected: 5, actual: 3 }
        let frame = vec![0xA8, 0x00, 0xA1];
        assert_error(frame, |e| {
            assert!(matches!(
                e,
                CommandResponseDecoderError::CommandResponseParseError(
                    CommandResponseParseError::InvalidFrameLength {
                        expected: 5,
                        actual: 3
                    }
                )
            ));
        });
    }

    // --- update() unit tests ---

    // A non-A0 first byte (e.g. 0xA1) does not set expected_length yet, so update returns Ok(None).
    #[test]
    fn test_update_non_a0_first_byte_returns_none() {
        let mut decoder = CommandResponseDecoder::new();
        assert_eq!(decoder.update(0xA1).unwrap(), None);
    }

    // A 0xFF byte in the middle of a frame triggers escaping and returns Ok(None)
    // without pushing any byte or advancing the frame length.
    #[test]
    fn test_update_ff_returns_none_and_does_not_advance() {
        let mut decoder = CommandResponseDecoder::new();
        decoder.update(0xA1).unwrap(); // start frame
        assert_eq!(decoder.update(0xFF).unwrap(), None);
    }

    // After a 0xFF escape byte, the following byte is stored as its bitwise inverse (!byte).
    // Wire frame [0xAA, 0xAA, 0x00, 0x00, 0x01, 0x00, 0x01, 0xFF, 0x00, 0x01] decodes as
    // Pong { sequence_number: 0x00, pong_value: 0xFF } because 0xFF in the payload is
    // transmitted as the escape pair [0xFF, 0x00] (!0x00 = 0xFF).
    #[test]
    fn test_update_ff_escape_decodes_as_inverted_byte() {
        let wire_frame = vec![0xAA, 0xAA, 0x00, 0x00, 0x01, 0x00, 0x01, 0xFF, 0x00, 0x01];
        assert_valid_response(
            wire_frame,
            CommandResponse::Pong {
                sequence_number: 0x00,
                pong_value: 0xFF,
            },
        );
    }

    // After a complete frame is returned, the decoder resets its internal state and can
    // immediately receive and decode a second frame.
    #[test]
    fn test_update_decoder_reusable_after_completion() {
        let mut decoder = CommandResponseDecoder::new();

        // First frame
        assert_eq!(
            decoder.update(0xA0).unwrap(),
            Some(CommandResponse::ReadyForNextFlashBlock)
        );

        // Second frame — decoder must have auto-reset
        assert_eq!(
            decoder.update(0xA0).unwrap(),
            Some(CommandResponse::ReadyForNextFlashBlock)
        );
    }

    // --- reset() unit tests ---

    // reset() in the middle of a partial frame discards accumulated bytes so the decoder
    // can accept a fresh frame.
    #[test]
    fn test_reset_clears_partial_frame() {
        let mut decoder = CommandResponseDecoder::new();
        decoder.update(0xA1).unwrap(); // start an A1 frame, not complete yet
        decoder.reset();
        // After reset, a fresh A0 frame must be accepted and decoded correctly.
        assert_eq!(
            decoder.update(0xA0).unwrap(),
            Some(CommandResponse::ReadyForNextFlashBlock)
        );
    }

    // reset() while the decoder is in escaping state (after receiving a 0xFF byte) clears
    // the escaping flag so the next start byte is not misinterpreted.
    #[test]
    fn test_reset_clears_escaping_state() {
        let mut decoder = CommandResponseDecoder::new();
        decoder.update(0xA1).unwrap(); // start frame
        decoder.update(0xFF).unwrap(); // enter escaping state
        decoder.reset();
        // After reset, a fresh A0 frame must decode normally (not treat 0xA0 as an escaped byte).
        assert_eq!(
            decoder.update(0xA0).unwrap(),
            Some(CommandResponse::ReadyForNextFlashBlock)
        );
    }

    #[test]
    fn test_multiple_chained_command_responses() {
        let mut decoder = CommandResponseDecoder::new();
        let frame1 = vec![0xA1, 0xAC];
        let frame2 = vec![0xA8, 0x00, 0x00, 0x00, 0x00];
        let frame3 = vec![0xA1, 0xAF];
        for i in 0..frame1.len() {
            let response = decoder.update(frame1[i]).unwrap();
            if response.is_some() {
                assert!(i == frame1.len() - 1);
                assert_eq!(response, Some(CommandResponse::InFlashingModeOk));
                break;
            } else {
                assert!(i < frame1.len() - 1);
            }
        }
        for i in 0..frame2.len() {
            let response = decoder.update(frame2[i]).unwrap();
            if response.is_some() {
                assert!(i == frame2.len() - 1);
                assert_eq!(
                    response,
                    Some(CommandResponse::StartWriteOk { offset: 0x00000000 })
                );
                break;
            } else {
                assert!(i < frame2.len() - 1);
            }
        }
        for i in 0..frame3.len() {
            let response = decoder.update(frame3[i]).unwrap();
            if response.is_some() {
                assert!(i == frame3.len() - 1);
                assert_eq!(response, Some(CommandResponse::InFlashingModeNotOk));
                break;
            } else {
                assert!(i < frame3.len() - 1);
            }
        }
    }
}
