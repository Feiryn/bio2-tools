use protocol::{
    frame::{
        command_response::{CommandResponse, CommandResponseParseError},
        command_response_decoder::{CommandResponseDecoder, CommandResponseDecoderError},
        flash_command::FlashCommand,
        host_command::HostCommand,
    },
    reader::bio2_reader::{Bio2Reader, Bio2ReaderError},
};
use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum FlashState {
    Init,
    ReconnectAfterReboot,
    PingSetup,
    Ping,
    IdentSetup,
    Ident,
    EnterFlashModeSetup,
    EnterFlashMode,
    VerifyFlashModeSetup,
    VerifyFlashMode,
    StartWritingSetup,
    StartWriting,
    VerifyFlashMode2Setup,
    VerifyFlashMode2,
    BlockWriteSetup,
    BlockWrite,
    StopWritingSetup,
    StopWriting,
    StopWriting2Setup,
    StopWriting2,
    Reboot,
    IdentNewFirmwareSetup,
    IdentNewFirmware,
    Finish,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum FlashStateMachineError {
    #[error("Failed to reconnect to the device after rebooting")]
    ReconnectFailed,
    #[error(
        "Ping command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    PingCommandSetupInvalidState,
    #[error("Failed to send ping command to the device")]
    PingSendFailed(Bio2ReaderError),
    #[error("Failed to read response from the device within the timeout")]
    ResponseTimeout,
    #[error("Failed to parse response from the device: {0}")]
    ResponseParseError(CommandResponseParseError),
    #[error("Received unknown response from the device")]
    UnknownResponse,
    #[error("Received unexpected response from the device: {0:?}")]
    UnexpectedResponse(CommandResponse),
    #[error("Received response with wrong sequence number: expected {expected}, got {got}")]
    WrongSequenceNumber { expected: u8, got: u8 },
    #[error(
        "Received pong response with wrong pong value: expected {expected:#04x}, got {got:#04x}"
    )]
    WrongPongValue { expected: u8, got: u8 },
    #[error(
        "Ident command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    IdentCommandSetupInvalidState,
    #[error("Failed to send ident command to the device")]
    IdentSendFailed(Bio2ReaderError),
    #[error("Failed to send start flash command to the device")]
    StartFlashSendFailed(Bio2ReaderError),
    #[error(
        "Start flash command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    StartFlashCommandSetupInvalidState,
    #[error("Failed to enter flash mode: device did not acknowledge start flash command")]
    EnterFlashModeFailed,
    #[error("Failed to send verify flash mode command to the device")]
    VerifyFlashModeSendFailed(Bio2ReaderError),
    #[error(
        "Verify flash mode command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    VerifyFlashModeCommandSetupInvalidState,
    #[error("Failed to send start writing command to the device")]
    StartWritingSendFailed(Bio2ReaderError),
    #[error(
        "Start writing command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    StartWritingCommandSetupInvalidState,
    #[error("Board is not ready to receive the next command")]
    BoardNotReady,
    #[error("Data block is not 128 bytes long. It should never happen")]
    InvalidBlockDataLength,
    #[error("Failed to send flash block data to the device")]
    FlashBlockSendFailed(Bio2ReaderError),
    #[error("Device did not acknowledge flash block command with an ok response")]
    InvalidBlockWriteResponse,
    #[error(
        "Write block response firmware offset does not match the expected offset: expected {expected:#06x}, got {got:#06x}"
    )]
    WriteBlockOffsetMismatch { expected: u32, got: u32 },
    #[error(
        "Write block response data does not match the expected data for the block offset: expected {expected:?}, got {got:?}"
    )]
    WriteBlockDataMismatch { expected: Vec<u8>, got: Vec<u8> },
    #[error("Failed to send stop writing command to the device")]
    StopWritingSendFailed(Bio2ReaderError),
    #[error(
        "Stop writing command setup is in invalid state. It shouldn't happen, this is a bug in the flashing state machine implementation."
    )]
    StopWritingCommandSetupInvalidState,
    #[error("Device did not acknowledge end writing command with an ok response")]
    EndWritingFailed,
    #[error("Failed to reboot the device")]
    RebootFailed(Bio2ReaderError),
}

pub struct FlashStateMachine {
    pub state: FlashState,
    pub reader: Box<dyn Bio2Reader>,
    firmware: Vec<u8>,
    retries_countdown: i32,
    ping_command: Option<HostCommand>,
    ident_command: Option<HostCommand>,
    enter_flash_mode_command: Option<HostCommand>,
    verify_flash_mode_command: Option<FlashCommand>,
    start_writing_command: Option<FlashCommand>,
    verify_flash_mode_command_2: Option<FlashCommand>,
    stop_writing_command: Option<FlashCommand>,
    curr_block_offset: u32,
    curr_sequence: u8,
    last_block_write_was_error: bool,
    response_decoder: CommandResponseDecoder,
    is_128kb: bool,
}

impl FlashStateMachine {
    pub fn new_64kb(reader: Box<dyn Bio2Reader>, firmware: [u8; 0x10000]) -> Self {
        FlashStateMachine {
            state: FlashState::Init,
            reader,
            firmware: firmware.to_vec(),
            retries_countdown: 0,
            ping_command: None,
            ident_command: None,
            enter_flash_mode_command: None,
            verify_flash_mode_command: None,
            start_writing_command: None,
            verify_flash_mode_command_2: None,
            stop_writing_command: None,
            curr_sequence: 0,
            curr_block_offset: 0,
            last_block_write_was_error: false,
            response_decoder: CommandResponseDecoder::new(),
            is_128kb: false,
        }
    }

    pub fn new_128kb(reader: Box<dyn Bio2Reader>, firmware: [u8; 0x20000]) -> Self {
        FlashStateMachine {
            state: FlashState::Init,
            reader,
            firmware: firmware.to_vec(),
            retries_countdown: 0,
            ping_command: None,
            ident_command: None,
            enter_flash_mode_command: None,
            verify_flash_mode_command: None,
            start_writing_command: None,
            verify_flash_mode_command_2: None,
            stop_writing_command: None,
            curr_sequence: 0,
            curr_block_offset: 0,
            last_block_write_was_error: false,
            response_decoder: CommandResponseDecoder::new(),
            is_128kb: true,
        }
    }

    fn read_next_response(&mut self) -> Result<CommandResponse, FlashStateMachineError> {
        while let Ok(Some(byte)) = self.reader.read_byte() {
            match self.response_decoder.update(byte) {
                Ok(Some(response)) => return Ok(response),
                Ok(None) => continue,
                Err(e) => match e {
                    CommandResponseDecoderError::UnknownStartByte(_) => {
                        eprintln!(
                            "Received unknown byte while waiting for response: 0x{:02x}",
                            byte
                        );
                        return Err(FlashStateMachineError::UnknownResponse);
                    }
                    CommandResponseDecoderError::CommandResponseParseError(parse_error) => {
                        eprintln!("Failed to parse response: {}", parse_error);
                        return Err(FlashStateMachineError::ResponseParseError(parse_error));
                    }
                },
            }
        }

        Err(FlashStateMachineError::ResponseTimeout)
    }

    fn wait_ready(&mut self) -> Result<(), FlashStateMachineError> {
        match self.read_next_response() {
            Ok(response) => match response {
                CommandResponse::ReadyForNextFlashBlock => Ok(()),
                other => Err(FlashStateMachineError::UnexpectedResponse(other)),
            },
            Err(e) => match e {
                FlashStateMachineError::ResponseTimeout => {
                    Err(FlashStateMachineError::BoardNotReady)
                }
                _ => Err(e),
            },
        }
    }

    fn step_init(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Starting flashing process...");
        self.state = FlashState::PingSetup;
        Ok(())
    }

    fn step_reconnect_after_reboot(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Waiting for the device to reboot...");
        std::thread::sleep(std::time::Duration::from_millis(4000));
        let reconnect_result = self.reader.reconnect();
        if let Err(e) = reconnect_result {
            eprintln!("Failed to reconnect to the device: {}", e);
            self.retries_countdown -= 1;
            if self.retries_countdown <= 0 {
                return Err(FlashStateMachineError::ReconnectFailed);
            }
            Ok(())
        } else {
            println!("Reconnected to the device successfully!");
            self.state = FlashState::IdentNewFirmwareSetup;
            Ok(())
        }
    }

    fn step_ping_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Pinging the device...");
        self.state = FlashState::Ping;
        self.ping_command = Some(HostCommand::Ping { ping_value: 0x00 });
        self.retries_countdown = 3;
        Ok(())
    }

    fn step_ping(&mut self) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.ping_command {
            // Get the current sequence number and increment it for the next command
            let sequence = self.curr_sequence;
            self.curr_sequence = self.curr_sequence.wrapping_add(1);

            // Send the ping to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame(sequence)) {
                Err(FlashStateMachineError::PingSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Pong response received
                    Ok(CommandResponse::Pong {
                        pong_value,
                        sequence_number,
                    }) => {
                        if sequence != sequence_number {
                            Err(FlashStateMachineError::WrongSequenceNumber {
                                expected: sequence,
                                got: sequence_number,
                            })
                        } else if pong_value != 0x01 {
                            Err(FlashStateMachineError::WrongPongValue {
                                expected: 0x01,
                                got: pong_value,
                            })
                        } else {
                            println!("Received valid pong response from the device!");
                            //self.state = FlashState::IdentSetup;
                            self.state = FlashState::EnterFlashModeSetup; // todo change that
                            Ok(())
                        }
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::PingSetup;
            Err(FlashStateMachineError::PingCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to ping the device: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the Ping state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to ping the device");
                return res;
            } else {
                eprintln!(
                    "Failed to ping the device, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_ident_setup(&mut self, next_state: FlashState) -> Result<(), FlashStateMachineError> {
        println!("Identifying the device...");
        self.ident_command = Some(HostCommand::Version);
        self.retries_countdown = 3;

        if next_state == FlashState::IdentNewFirmware {
            self.curr_sequence = 0;
        }

        self.state = next_state;

        Ok(())
    }

    fn step_ident(&mut self, next_state: FlashState) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.ident_command {
            // Get the current sequence number and increment it for the next command
            let sequence = self.curr_sequence;
            self.curr_sequence = self.curr_sequence.wrapping_add(1);

            // Send the ident command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame(sequence)) {
                Err(FlashStateMachineError::IdentSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Version response received
                    Ok(CommandResponse::Version {
                        revision,
                        major,
                        minor,
                        patch,
                        name,
                        date,
                        time,
                        sequence_number,
                    }) => {
                        if sequence != sequence_number {
                            Err(FlashStateMachineError::WrongSequenceNumber {
                                expected: sequence,
                                got: sequence_number,
                            })
                        } else {
                            println!(
                                "Received version response from the device: {} v{}.{}.{} rev {} - {} {}",
                                name, major, minor, patch, revision, date, time
                            );
                            self.state = next_state;
                            Ok(())
                        }
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::IdentSetup;
            Err(FlashStateMachineError::IdentCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to identify the device: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the Ping state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to identify the device");
                return res;
            } else {
                println!(
                    "Failed to identify the device, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_enter_flash_mode_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Entering flash mode...");
        self.state = FlashState::EnterFlashMode;
        self.enter_flash_mode_command = Some(HostCommand::StartFlash);
        self.retries_countdown = 3;
        Ok(())
    }

    fn step_enter_flash_mode(&mut self) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.enter_flash_mode_command {
            // Get the current sequence number and increment it for the next command
            let sequence = self.curr_sequence;
            self.curr_sequence = self.curr_sequence.wrapping_add(1);

            // Send the start flash command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame(sequence)) {
                Err(FlashStateMachineError::StartFlashSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Start flash ack response received
                    Ok(CommandResponse::StartFlash {
                        sequence_number,
                        ok,
                    }) => {
                        if sequence != sequence_number {
                            Err(FlashStateMachineError::WrongSequenceNumber {
                                expected: sequence,
                                got: sequence_number,
                            })
                        } else if !ok {
                            eprintln!("Device did not acknowledge start flash command");
                            Err(FlashStateMachineError::EnterFlashModeFailed)
                        } else {
                            println!("Device acknowledged start flash command, now in flash mode!");
                            match self.wait_ready() {
                                Ok(()) => {
                                    println!("Device is ready for the next step!");
                                    self.state = FlashState::VerifyFlashModeSetup;
                                    Ok(())
                                }
                                Err(e) => Err(e),
                            }
                        }
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::EnterFlashModeSetup;
            Err(FlashStateMachineError::StartFlashCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to enter flash mode: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the EnterFlashMode state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to enter flash mode");
                return res;
            } else {
                println!(
                    "Failed to enter flash mode, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_verify_flash_mode_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Verifying flash mode...");
        self.state = FlashState::VerifyFlashMode;
        self.verify_flash_mode_command = Some(FlashCommand::AreYouInFlashMode);
        self.retries_countdown = 3;
        Ok(())
    }

    fn step_verify_flash_mode(&mut self) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.verify_flash_mode_command {
            // Send the "are you in flash mode" command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame()) {
                Err(FlashStateMachineError::VerifyFlashModeSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Device is in flash mode
                    Ok(CommandResponse::InFlashingModeOk) => {
                        println!("Device is in flash mode!");

                        match self.wait_ready() {
                            Ok(()) => {
                                println!("Device is ready for the next step!");
                                self.state = FlashState::StartWritingSetup;
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    // Device is not in flash mode. Currently I don't want to go back to the previous step because idk the board behaviour
                    Ok(CommandResponse::InFlashingModeNotOk) => {
                        eprintln!("Device is not in flash mode");
                        Err(FlashStateMachineError::EnterFlashModeFailed)
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::VerifyFlashModeSetup;
            Err(FlashStateMachineError::VerifyFlashModeCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to verify flash mode: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the VerifyFlashMode state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to verify flash mode");
                return res;
            } else {
                println!(
                    "Failed to verify flash mode, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_start_writing_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Starting flash writing process...");
        self.state = FlashState::StartWriting;
        self.start_writing_command = Some(FlashCommand::StartWrite { offset: 0 });
        self.retries_countdown = 3;
        Ok(())
    }

    fn step_start_writing(&mut self) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.start_writing_command {
            // Send the start writing command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame()) {
                Err(FlashStateMachineError::StartWritingSendFailed(e))
            } else {
                let response: Result<CommandResponse, FlashStateMachineError> =
                    self.read_next_response();
                match response {
                    // Start writing ack response received
                    Ok(CommandResponse::StartWriteOk { offset: _ }) => {
                        println!("Device acknowledged start writing command");

                        match self.wait_ready() {
                            Ok(()) => {
                                println!("Device is ready for the next step!");
                                self.state = FlashState::VerifyFlashMode2Setup;
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::StartWritingSetup;
            Err(FlashStateMachineError::StartWritingCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to start flash writing process: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the StartWriting state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to start flash writing process");
                return res;
            } else {
                println!(
                    "Failed to start flash writing process, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_verify_flash_mode2_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Verifying flash mode again before starting to send flash blocks...");
        self.state = FlashState::VerifyFlashMode2;
        self.verify_flash_mode_command_2 = Some(FlashCommand::AreYouInFlashMode);
        self.retries_countdown = 3;
        Ok(())
    }

    fn step_verify_flash_mode2(&mut self) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.verify_flash_mode_command {
            // Send the "are you in flash mode" command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame()) {
                Err(FlashStateMachineError::VerifyFlashModeSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Device is in flash mode
                    Ok(CommandResponse::InFlashingModeOk) => {
                        println!("Device is in flash mode!");

                        match self.wait_ready() {
                            Ok(()) => {
                                println!("Device is ready for the next step!");
                                self.state = FlashState::BlockWriteSetup;
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    // Device is not in flash mode. Currently I don't want to go back to the previous step because idk the board behaviour
                    Ok(CommandResponse::InFlashingModeNotOk) => {
                        eprintln!("Device is not in flash mode");
                        Err(FlashStateMachineError::EnterFlashModeFailed)
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::VerifyFlashMode2Setup;
            Err(FlashStateMachineError::VerifyFlashModeCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to verify flash mode: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the VerifyFlashMode2 state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to verify flash mode");
                return res;
            } else {
                println!(
                    "Failed to verify flash mode, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_block_write_setup(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Starting to send flash blocks...");
        self.state = FlashState::BlockWrite;
        self.retries_countdown = 3;
        self.curr_block_offset = if self.is_128kb { 0x20000 } else { 0x10000 };
        self.last_block_write_was_error = false;
        Ok(())
    }

    fn step_block_write(&mut self) -> Result<(), FlashStateMachineError> {
        let res = {
            self.curr_block_offset -= 128;

            let block_data = &self.firmware
                [self.curr_block_offset as usize..(self.curr_block_offset as usize + 128)];

            match TryInto::<[u8; 128]>::try_into(block_data) {
                Ok(block_data) => {
                    let write_block_command = FlashCommand::WriteBlock {
                        firmware_offset: self.curr_block_offset,
                        block_data: block_data.try_into().unwrap(),
                        is_128kb_firmware: self.is_128kb,
                    };

                    let frame = write_block_command.to_frame();

                    if let Err(e) = self.reader.write_bytes(&frame) {
                        Err(FlashStateMachineError::FlashBlockSendFailed(e))
                    } else {
                        let response = self.read_next_response();
                        match response {
                            Ok(CommandResponse::WriteBlockOk {
                                firmware_offset: resp_offset,
                                block_data: resp_block_data,
                            }) => {
                                let resp_offset_shift = if self.is_128kb {
                                    (resp_offset as u32) << 1
                                } else {
                                    resp_offset as u32
                                };
                                if resp_offset_shift != self.curr_block_offset {
                                    Err(FlashStateMachineError::WriteBlockOffsetMismatch {
                                        expected: self.curr_block_offset,
                                        got: resp_offset_shift,
                                    })
                                } else if resp_block_data != block_data {
                                    Err(FlashStateMachineError::WriteBlockDataMismatch {
                                        expected: block_data.to_vec(),
                                        got: resp_block_data.to_vec(),
                                    })
                                } else {
                                    let firmware_size =
                                        if self.is_128kb { 0x20000 } else { 0x10000 };
                                    let percentage = ((firmware_size - self.curr_block_offset)
                                        as f32
                                        / firmware_size as f32)
                                        * 100.0;
                                    if self.curr_block_offset != 0xff80
                                        && self.curr_block_offset != 0x1ff80
                                        && !self.last_block_write_was_error
                                    {
                                        print!(" | {}%", percentage as u8);
                                    } else {
                                        print!("{}%", percentage as u8);
                                    }
                                    self.last_block_write_was_error = false;

                                    match self.wait_ready() {
                                        Ok(()) => {
                                            if self.curr_block_offset == 0 {
                                                println!("");
                                                println!("All firmware blocks sent successfully!");
                                                self.state = FlashState::StopWritingSetup;
                                            }

                                            Ok(())
                                        }
                                        Err(e) => Err(e),
                                    }
                                }
                            }
                            Ok(CommandResponse::WriteBlockNotOk) => {
                                eprintln!("Device did not acknowledge write block command");
                                Err(FlashStateMachineError::InvalidBlockWriteResponse)
                            }
                            Ok(other_response) => {
                                Err(FlashStateMachineError::UnexpectedResponse(other_response))
                            }
                            Err(e) => Err(e),
                        }
                    }
                }
                Err(_) => Err(FlashStateMachineError::InvalidBlockDataLength),
            }
        };

        if let Err(e) = &res {
            if self.curr_block_offset != 0xff80 && !self.last_block_write_was_error {
                println!("");
            }
            eprintln!(
                "Failed to send flash block at offset 0x{:04x}: {}",
                self.curr_block_offset, e
            );

            self.curr_block_offset += 128;
            self.last_block_write_was_error = true;
            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the BlockWrite state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to send flash block");
                return res;
            } else {
                println!(
                    "Failed to send flash block, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_stop_writing_setup(&mut self, state: FlashState) -> Result<(), FlashStateMachineError> {
        println!("Finishing flash writing process...");
        self.state = state;
        self.retries_countdown = 3;
        self.stop_writing_command = Some(FlashCommand::EndWrite);
        Ok(())
    }

    fn step_stop_writing(&mut self, next_state: FlashState) -> Result<(), FlashStateMachineError> {
        let res = if let Some(cmd) = &self.stop_writing_command {
            // Send the stop writing command to the device
            if let Err(e) = self.reader.write_bytes(&cmd.to_frame()) {
                Err(FlashStateMachineError::StopWritingSendFailed(e))
            } else {
                let response = self.read_next_response();
                match response {
                    // Stop writing ack response received
                    Ok(CommandResponse::EndWriteOk) => {
                        println!("Device acknowledged end writing command");

                        match self.wait_ready() {
                            Ok(()) => {
                                println!("Device is ready for the next step!");
                                self.state = next_state;
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    // Device did not acknowledge end writing command
                    Ok(CommandResponse::EndWriteNotOk) => {
                        Err(FlashStateMachineError::EndWritingFailed)
                    }
                    // Some other response received
                    Ok(other_response) => {
                        Err(FlashStateMachineError::UnexpectedResponse(other_response))
                    }
                    // Error while reading response
                    Err(e) => Err(e),
                }
            }
        } else {
            self.state = FlashState::StopWritingSetup;
            Err(FlashStateMachineError::StopWritingCommandSetupInvalidState)
        };

        if let Err(e) = &res {
            eprintln!("Failed to finish flash writing process: {}", e);

            self.reader.clear();

            // Decrement retries
            self.retries_countdown -= 1;

            // If no retries left, return the error. Otherwise, print a warning and stay in the StopWriting state to retry
            if self.retries_countdown <= 0 {
                eprintln!("Failed to finish flash writing process");
                return res;
            } else {
                println!(
                    "Failed to finish flash writing process, retrying... ({} retries left)",
                    self.retries_countdown
                );
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn step_reboot(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Rebooting the device...");
        self.state = FlashState::ReconnectAfterReboot;
        self.reader
            .write_bytes(&FlashCommand::Reboot.to_frame())
            .map_err(FlashStateMachineError::RebootFailed)
    }

    fn step_finish(&mut self) -> Result<(), FlashStateMachineError> {
        println!("Flashing process finished successfully!");
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), FlashStateMachineError> {
        match self.state {
            FlashState::Init => self.step_init(),
            FlashState::ReconnectAfterReboot => self.step_reconnect_after_reboot(),
            FlashState::PingSetup => self.step_ping_setup(),
            FlashState::Ping => self.step_ping(),
            FlashState::IdentSetup => self.step_ident_setup(FlashState::Ident),
            FlashState::Ident => self.step_ident(FlashState::EnterFlashModeSetup),
            FlashState::EnterFlashModeSetup => self.step_enter_flash_mode_setup(),
            FlashState::EnterFlashMode => self.step_enter_flash_mode(),
            FlashState::VerifyFlashModeSetup => self.step_verify_flash_mode_setup(),
            FlashState::VerifyFlashMode => self.step_verify_flash_mode(),
            FlashState::StartWritingSetup => self.step_start_writing_setup(),
            FlashState::StartWriting => self.step_start_writing(),
            FlashState::VerifyFlashMode2Setup => self.step_verify_flash_mode2_setup(),
            FlashState::VerifyFlashMode2 => self.step_verify_flash_mode2(),
            FlashState::BlockWriteSetup => self.step_block_write_setup(),
            FlashState::BlockWrite => self.step_block_write(),
            FlashState::StopWritingSetup => self.step_stop_writing_setup(FlashState::StopWriting),
            FlashState::StopWriting => self.step_stop_writing(FlashState::StopWriting2Setup),
            FlashState::StopWriting2Setup => self.step_stop_writing_setup(FlashState::StopWriting2),
            FlashState::StopWriting2 => self.step_stop_writing(FlashState::Reboot),
            FlashState::Reboot => self.step_reboot(),
            FlashState::IdentNewFirmwareSetup => {
                self.step_ident_setup(FlashState::IdentNewFirmware)
            }
            FlashState::IdentNewFirmware => self.step_ident(FlashState::Finish),
            FlashState::Finish => self.step_finish(),
        }
    }
}
