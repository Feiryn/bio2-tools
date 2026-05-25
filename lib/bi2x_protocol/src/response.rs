use thiserror::Error;

use crate::frame::Bi2xFrame;

/// Represents a high-level response parsed from a Bi2xFrame.
#[derive(Debug)]
pub enum Bi2xResponse {
    /// Response to a Ping command
    Pong,
    /// Response to an Initialize command, indicating whether initialization was successful.
    /// It can not be successful if the board is already initialized.
    InitializationResult { success: bool },
    /// Response to a GetFirmwareVersion command, containing the raw bytes of the firmware version information returned by the board.
    FirmwareVersion {
        success: bool,
        major: u8,
        minor: u8,
        patch: u8,
        name: String,
        timestamp_ms: u64,
    },
    /// Response to an AllocateMemoryForFirmwareUpload command, indicating whether the memory allocation was successful and providing the handle for the allocated memory if successful.
    MemoryAllocationResult { success: bool, handle: u8 },
    /// Response to an UploadFirmwareChunk command, indicating whether the firmware chunk upload was successful.
    FirmwareUploadChunkResult { success: bool },
    /// Response to a FinishFirmwareUploadAndInit command, indicating whether the firmware upload and initialization was successful and providing the handle for the initialized firmware if successful.
    FirmwareUploadFinishResult { success: bool, handle: u8 },
    /// Represents an error response from the board, containing the error code.
    Error { code: u8 },
}

#[derive(Debug, Error)]
pub enum FrameToResponseError {
    #[error("Unexpected end of payload while parsing response")]
    UnexpectedEndOfPayload,
    #[error("Unknown response type {response_type} for node {node}")]
    UnknownResponseType { node: u8, response_type: u8 },
    #[error("Unexpected handle {handle}")]
    UnexpectedHandle { handle: u8 },
    #[error("Unknown node {node}")]
    UnknownNode { node: u8 },
}

impl Bi2xResponse {
    fn from_bytes(node: u8, bytes: &[u8]) -> Result<(Bi2xResponse, usize), FrameToResponseError> {
        let mut consumed_bytes = 0;

        if bytes.len() == 1 {
            return Err(FrameToResponseError::UnexpectedEndOfPayload);
        }

        match node {
            0x00 => {
                if bytes.len() == 0 {
                    return Ok((Bi2xResponse::Pong, consumed_bytes));
                }

                if bytes.len() < 3 {
                    return Err(FrameToResponseError::UnexpectedEndOfPayload);
                }

                if bytes[0] == 0x00 && bytes[1] == 0x01 {
                    let success = bytes[2] == 0x00;
                    consumed_bytes += 3;
                    return Ok((
                        Bi2xResponse::InitializationResult { success },
                        consumed_bytes,
                    ));
                }

                return Err(FrameToResponseError::UnknownResponseType {
                    node,
                    response_type: bytes[1],
                });
            }
            0x03 => match bytes[1] {
                0x02 => {
                    if bytes.len() < 35 {
                        return Err(FrameToResponseError::UnexpectedEndOfPayload);
                    }

                    if bytes[0] != 0x00 {
                        return Err(FrameToResponseError::UnexpectedHandle { handle: bytes[0] });
                    }

                    let success = bytes[2] == 0x00;

                    let major = bytes[8];
                    let minor = bytes[9];
                    let patch = bytes[10];

                    let name_bytes = &bytes[11..15];
                    let name = String::from_utf8_lossy(name_bytes).to_string();

                    let timestamp_ms = ((bytes[15] as u64) << 24
                        | (bytes[16] as u64) << 16
                        | (bytes[17] as u64) << 8
                        | bytes[18] as u64)
                        * 60000;

                    consumed_bytes += 35;

                    Ok((
                        Bi2xResponse::FirmwareVersion {
                            success,
                            major,
                            minor,
                            patch,
                            name,
                            timestamp_ms,
                        },
                        consumed_bytes,
                    ))
                }
                0x10 => {
                    if bytes.len() < 4 {
                        return Err(FrameToResponseError::UnexpectedEndOfPayload);
                    }

                    if bytes[0] != 0x00 {
                        return Err(FrameToResponseError::UnexpectedHandle { handle: bytes[0] });
                    }

                    let success = bytes[2] == 0x00;
                    let allocated_handle = bytes[3];

                    consumed_bytes += 4;
                    Ok((
                        Bi2xResponse::MemoryAllocationResult {
                            success,
                            handle: allocated_handle,
                        },
                        consumed_bytes,
                    ))
                }
                0x13 => {
                    if bytes.len() < 3 {
                        return Err(FrameToResponseError::UnexpectedEndOfPayload);
                    }

                    if bytes[0] != 0x00 {
                        return Err(FrameToResponseError::UnexpectedHandle { handle: bytes[0] });
                    }

                    let success = bytes[2] == 0x00;
                    consumed_bytes += 3;
                    Ok((
                        Bi2xResponse::FirmwareUploadChunkResult { success },
                        consumed_bytes,
                    ))
                }
                0x78 => {
                    if bytes.len() < 4 {
                        return Err(FrameToResponseError::UnexpectedEndOfPayload);
                    }

                    if bytes[0] != 0x00 {
                        return Err(FrameToResponseError::UnexpectedHandle { handle: bytes[0] });
                    }

                    let success = bytes[2] == 0x00;
                    let firmware_handle = bytes[3];

                    consumed_bytes += 4;
                    Ok((
                        Bi2xResponse::FirmwareUploadFinishResult {
                            success,
                            handle: firmware_handle,
                        },
                        consumed_bytes,
                    ))
                }
                _ => Err(FrameToResponseError::UnknownResponseType {
                    node,
                    response_type: bytes[1],
                }),
            },
            _ => Err(FrameToResponseError::UnknownNode { node }),
        }
    }

    /// Parses a Bi2xFrame into one or more Bi2xResponse objects, depending on the contents of the frame's payload.
    /// A single frame may contain multiple responses concatenated together.
    pub fn from_frame(frame: Bi2xFrame) -> Result<Vec<Bi2xResponse>, FrameToResponseError> {
        let mut responses = Vec::new();

        let payload_length = frame.payload.as_ref().map(|p| p.len()).unwrap_or(0);
        let payload = frame.payload.unwrap_or(vec![]).to_owned();

        let (first_response, mut consumed_bytes) =
            Bi2xResponse::from_bytes(frame.node, payload.as_slice())?;

        responses.push(first_response);

        while consumed_bytes < payload_length {
            let (response, bytes_consumed) =
                Bi2xResponse::from_bytes(frame.node, &payload[consumed_bytes..])?;

            responses.push(response);
            consumed_bytes += bytes_consumed;
        }

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use crate::frame::Bi2xFrameMode;

    use super::*;

    #[test]
    fn test_parse_pong_response() {
        let frame = Bi2xFrame {
            node: 0x00,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(responses[0], Bi2xResponse::Pong));
    }

    #[test]
    fn test_parse_initialization_result_response() {
        let frame = Bi2xFrame {
            node: 0x00,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x01, 0x00]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::InitializationResult { success: true }
        ));

        let frame = Bi2xFrame {
            node: 0x00,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x01, 0x01]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::InitializationResult { success: false }
        ));
    }

    #[test]
    fn test_parse_firmware_version_response() {
        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![
                0x00, 0x02, 0x00, 0x0d, 0x06, 0x00, 0x01, 0x00, 0x01, 0x02, 0x09, 0x42, 0x49, 0x32,
                0x58, 0x01, 0x96, 0x3e, 0xc4, 0x00, 0x00, 0x01, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0xbb, 0x1f, 0x8e, 0xe2,
            ]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        if let Bi2xResponse::FirmwareVersion {
            success,
            major,
            minor,
            patch,
            name,
            timestamp_ms,
        } = &responses[0]
        {
            assert_eq!(major, &0x01);
            assert_eq!(minor, &0x02);
            assert_eq!(patch, &0x09);
            assert_eq!(name, "BI2X");
            assert_eq!(*timestamp_ms, 1597421040000);
            assert_eq!(*success, true);
        } else {
            panic!("Expected FirmwareVersion response");
        }
    }

    #[test]
    fn test_parse_memory_allocation_result_response() {
        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x10, 0x00, 0x02]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::MemoryAllocationResult {
                success: true,
                handle: 2
            }
        ));

        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x10, 0x01, 0x02]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::MemoryAllocationResult {
                success: false,
                handle: 2
            }
        ));
    }

    #[test]
    fn test_parse_firmware_upload_chunk_result_response() {
        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x13, 0x00]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::FirmwareUploadChunkResult { success: true }
        ));

        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x13, 0x01]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::FirmwareUploadChunkResult { success: false }
        ));
    }

    #[test]
    fn test_parse_firmware_upload_finish_result_response() {
        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x78, 0x00, 0x02]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::FirmwareUploadFinishResult {
                success: true,
                handle: 2
            }
        ));

        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![0x00, 0x78, 0x01, 0x02]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            Bi2xResponse::FirmwareUploadFinishResult {
                success: false,
                handle: 2
            }
        ));
    }

    #[test]
    fn test_parse_multiple_responses_in_single_frame() {
        let frame = Bi2xFrame {
            node: 0x03,
            sequence_number: 0,
            mode: Bi2xFrameMode::Raw,
            is_encrypted: false,
            payload: Some(vec![
                0x00, 0x02, 0x00, 0x0d, 0x06, 0x00, 0x01, 0x00, 0x01, 0x02, 0x09, 0x42, 0x49, 0x32,
                0x58, 0x01, 0x96, 0x3e, 0xc4, 0x00, 0x00, 0x01, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0xbb, 0x1f, 0x8e, 0xe2, 0x00, 0x10, 0x00, 0x02,
            ]),
        };

        let responses = Bi2xResponse::from_frame(frame).unwrap();
        assert_eq!(responses.len(), 2);

        if let Bi2xResponse::FirmwareVersion {
            success,
            major,
            minor,
            patch,
            name,
            timestamp_ms,
        } = &responses[0]
        {
            assert_eq!(major, &0x01);
            assert_eq!(minor, &0x02);
            assert_eq!(patch, &0x09);
            assert_eq!(name, "BI2X");
            assert_eq!(*timestamp_ms, 1597421040000);
            assert_eq!(*success, true);
        } else {
            panic!("Expected FirmwareVersion response");
        }

        assert!(matches!(
            responses[1],
            Bi2xResponse::MemoryAllocationResult {
                success: true,
                handle: 2
            }
        ));
    }
}
