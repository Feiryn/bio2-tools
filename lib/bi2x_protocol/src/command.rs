use thiserror::Error;

use crate::frame::Bi2xFrame;

/// Represents a high-level command that can be converted into a Bi2xFrame.
pub enum Bi2xCommand {
    /// Ping the board
    Ping,
    /// Initialize the board
    Init,
    /// Get the firmware version
    GetFirmwareVersion,
    /// Allocate memory on the board for firmware upload, of the specified size in bytes
    AllocateMemoryForFirmwareUpload { size: u32 },
    /// Upload a chunk of firmware data to the board, at the specified offset in bytes from the start of the firmware, using the given handle obtained from AllocateMemoryForFirmwareUpload
    /// Data length must be 64 bytes or less
    UploadFirmwareChunk {
        handle: u8,
        offset: u32,
        data: Vec<u8>,
    },
    /// Finish the firmware upload and initialize the firmware, using the given handle obtained from AllocateMemoryForFirmwareUpload, the offset in bytes from the start of the firmware where the firmware descriptor is located, and any additional initialization arguments required by the firmware
    FinishFirmwareUploadAndInit {
        handle: u8,
        firmware_descriptor_offset: u32,
        init_arguments: Vec<u8>,
    },
}

#[derive(Debug, Error)]
pub enum MultipleCommandsToFrameError {
    #[error("At least one command is required to create a frame")]
    NoCommands,
    #[error("Ping cannot be combined with other commands")]
    PingWithOtherCommands,
    #[error("Commands require different nodes")]
    DifferentNodes,
}

impl Bi2xCommand {
    fn requires_encryption(&self) -> bool {
        match self {
            Bi2xCommand::Ping => false,
            Bi2xCommand::Init => false,
            Bi2xCommand::GetFirmwareVersion => false,
            _ => true,
        }
    }

    fn node(&self) -> u8 {
        match self {
            Bi2xCommand::Ping => 0x00,
            Bi2xCommand::Init => 0x00,
            Bi2xCommand::GetFirmwareVersion => 0x02,
            Bi2xCommand::AllocateMemoryForFirmwareUpload { .. } => 0x02,
            Bi2xCommand::UploadFirmwareChunk { .. } => 0x02,
            Bi2xCommand::FinishFirmwareUploadAndInit { .. } => 0x02,
        }
    }

    fn payload_command(&self) -> Option<Vec<u8>> {
        match self {
            Bi2xCommand::Ping => None,
            Bi2xCommand::Init => Some(vec![0x00, 0x01]),
            Bi2xCommand::GetFirmwareVersion => Some(vec![0x00, 0x02, 0x81]),
            Bi2xCommand::AllocateMemoryForFirmwareUpload { size } => {
                let mut payload = vec![0x00, 0x10];
                payload.extend_from_slice(&size.to_be_bytes());
                Some(payload)
            }
            Bi2xCommand::UploadFirmwareChunk {
                handle,
                offset,
                data,
            } => {
                let mut payload = vec![0x00, 0x13];
                payload.push(*handle);
                payload.extend_from_slice(&offset.to_be_bytes());
                payload.extend_from_slice(data);

                Some(payload)
            }
            Bi2xCommand::FinishFirmwareUploadAndInit {
                handle,
                firmware_descriptor_offset,
                init_arguments,
            } => {
                let mut payload = vec![0x00, 0x78];
                payload.push(*handle);
                payload.extend_from_slice(&firmware_descriptor_offset.to_be_bytes());
                payload.extend_from_slice(init_arguments);

                Some(payload)
            }
        }
    }

    /// Converts this command into a Bi2xFrame with the given sequence number.
    pub fn to_frame(&self, sequence_number: u8) -> Bi2xFrame {
        Bi2xFrame::new(
            self.node(),
            sequence_number,
            self.requires_encryption(),
            self.payload_command(),
        )
    }

    /// Converts multiple commands into a single Bi2xFrame with the given sequence number.
    pub fn commands_to_frame(
        commands: &[Bi2xCommand],
        sequence_number: u8,
    ) -> Result<Bi2xFrame, MultipleCommandsToFrameError> {
        if commands.is_empty() {
            return Err(MultipleCommandsToFrameError::NoCommands);
        }

        if commands.len() > 1 && commands.iter().any(|cmd| matches!(cmd, Bi2xCommand::Ping)) {
            return Err(MultipleCommandsToFrameError::PingWithOtherCommands);
        }

        let requires_encryption = commands.iter().any(|cmd| cmd.requires_encryption());
        let node = commands[0].node();
        if commands.iter().any(|cmd| cmd.node() != node) {
            return Err(MultipleCommandsToFrameError::DifferentNodes);
        }

        let mut payload = Vec::new();
        for cmd in commands {
            if let Some(cmd_payload) = cmd.payload_command() {
                payload.extend_from_slice(&cmd_payload);
            }
        }

        Ok(Bi2xFrame::new(
            node,
            sequence_number,
            requires_encryption,
            Some(payload),
        ))
    }
}
