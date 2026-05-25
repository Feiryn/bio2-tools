use std::any::Any;

use crate::bio2_reader::{Bio2Reader, Bio2ReaderError};

pub struct Bi2aTestReader {
    data: Vec<u8>,
    will_timeout: bool,
    has_rebooted: bool,
    firmware_data_1: Vec<u8>,
    firmware_data_2: Vec<u8>,
    written_data: Vec<String>,
}

impl Bio2Reader for Bi2aTestReader {
    fn read_byte(&mut self) -> Result<Option<u8>, Bio2ReaderError> {
        if self.will_timeout {
            self.will_timeout = false;
            return Err(Bio2ReaderError::Raw("Simulated timeout".to_string()));
        }
        if let Some(byte) = self.data.first() {
            let byte = *byte;
            self.data.remove(0);
            Ok(Some(byte))
        } else {
            Ok(None)
        }
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<(), Bio2ReaderError> {
        let hex_string = data
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>()
            .join(" ");
        self.written_data.push(hex_string);

        let mut escaped_data = Vec::new();
        let mut escape_next = false;
        for i in 0..data.len() {
            let byte = data[i];
            if escape_next {
                escaped_data.push(!byte);
                escape_next = false;
            } else if byte == 0xFF {
                escape_next = true;
            } else {
                escaped_data.push(byte);
            }
        }

        if escaped_data[0] == 0xaa {
            let command = ((escaped_data[2] as u16) << 8) | (escaped_data[3] as u16);
            let sequence = escaped_data[4];
            let payload_size = escaped_data[5] as usize;

            let (payload, resp_addr) = match command {
                0x0001 => {
                    if payload_size != 1 {
                        (vec![], 0x00)
                    } else {
                        (vec![escaped_data[6] + 1], 0x00)
                    }
                }
                0x0002 => {
                    if !self.has_rebooted {
                        (
                            vec![
                                0x0D, 0x06, 0x00, 0x00, 0x00, 0x01, 0x02, 0x0e, 0x42, 0x49, 0x32,
                                0x41, 0x4f, 0x63, 0x74, 0x20, 0x33, 0x31, 0x20, 0x32, 0x30, 0x31,
                                0x38, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x39, 0x3a, 0x31, 0x31,
                                0x3a, 0x35, 0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                            ],
                            0x80,
                        )
                    } else {
                        (
                            vec![
                                0x0D, 0x06, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x42, 0x49, 0x32,
                                0x58, 0x41, 0x70, 0x72, 0x20, 0x32, 0x33, 0x20, 0x32, 0x30, 0x31,
                                0x39, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x30, 0x3A, 0x35, 0x35,
                                0x3A, 0x32, 0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                            ],
                            0x80,
                        )
                    }
                }
                0x0004 => (vec![0x00], 0x80),
                _ => (vec![], 0x00),
            };

            let mut response = vec![
                0xaa,
                0xaa,
                resp_addr,
                (command >> 8) as u8,
                (command & 0xff) as u8,
                sequence,
                payload.len() as u8,
            ];
            response.extend(payload);
            let checksum = response[2..]
                .iter()
                .fold(0u8, |acc, &b| acc.wrapping_add(b));
            response.push(checksum);

            if command == 0x0004 {
                response.push(0xa0);
            }

            self.data.extend(response);
        } else if escaped_data[0] == 0xa1 {
            self.data.push(0xa1);
            self.data.push(0xac);
            self.data.push(0xa0);
        } else if escaped_data[0] == 0xa8 {
            if escaped_data.len() == 1 {
                self.data.push(0xa8);
                self.data.push(0xac);
                self.data.push(0xa0);
            } else if escaped_data.len() == 3
                && escaped_data[0] == 0xa8
                && escaped_data[1] == 0xa8
                && escaped_data[2] == 0xa8
            {
                self.has_rebooted = true;
                self.will_timeout = true;
            } else if escaped_data.len() == 5 {
                self.data.push(0xa8);
                self.data.push(data[1]);
                self.data.push(data[2]);
                self.data.push(data[3]);
                self.data.push(data[4]);
                self.data.push(0xa0);
            } else {
                self.data.extend(data);
                self.data.push(0xaf);
                self.data.push(0xa0);
            }
        } else if escaped_data[0] == 0xa2 {
            let hi = escaped_data[2] ^ 0xfe;
            let lo = hi ^ escaped_data[3];
            let firmware_offset = ((hi as u16) << 8) | (lo as u16);

            let firmware_offset_checksum = escaped_data[4];
            let calculated_checksum =
                (0x4b + escaped_data[2] as u64 + escaped_data[3] as u64) % 256;

            if firmware_offset_checksum != calculated_checksum as u8 {
                self.data.extend(data);
                self.data.push(0xaf);
                self.data.push(0xa0);
                println!(
                    "Invalid firmware offset checksum: expected {:02X}, got {:02X}",
                    calculated_checksum, firmware_offset_checksum
                );
                return Ok(());
            }

            let mut key_byte = lo ^ escaped_data[4];
            let mut decoded_block = [0u8; 128];
            let mut checksum: u64 = 0;

            for i in 0..64 {
                let enc1 = escaped_data[5 + i * 2] ^ key_byte;
                let enc2 = escaped_data[6 + i * 2] ^ enc1;
                key_byte = enc2;
                decoded_block[i * 2] = enc1;
                decoded_block[i * 2 + 1] = enc2;
                checksum += escaped_data[5 + i * 2] as u64 + escaped_data[6 + i * 2] as u64;
            }

            let final_checksum = (checksum % 256) as u8;

            if final_checksum != escaped_data[133] {
                self.data.extend(data);
                self.data.push(0xaf);
                self.data.push(0xa0);
                return Ok(());
            }

            if firmware_offset % 0x80 == 0 {
                if self.firmware_data_1.len() < (firmware_offset as usize + 128) {
                    self.firmware_data_1
                        .resize(firmware_offset as usize + 128, 0);
                }
                self.firmware_data_1[firmware_offset as usize..firmware_offset as usize + 128]
                    .copy_from_slice(&decoded_block);
            } else {
                if self.firmware_data_2.len() < (firmware_offset as usize + 128) {
                    self.firmware_data_2
                        .resize(firmware_offset as usize + 128, 0);
                }
                self.firmware_data_2[(firmware_offset as usize)..(firmware_offset as usize + 128)]
                    .copy_from_slice(&decoded_block);
            }

            self.data.extend(data);
            self.data.push(0xac);
            self.data.push(0xa0);
        }

        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), Bio2ReaderError> {
        self.data.clear();
        self.will_timeout = false;
        Ok(())
    }

    fn clear(&mut self) -> () {
        self.data.clear();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Bi2aTestReader {
    pub fn new() -> Self {
        Bi2aTestReader {
            data: Vec::new(),
            will_timeout: false,
            has_rebooted: false,
            firmware_data_1: Vec::new(),
            firmware_data_2: Vec::new(),
            written_data: Vec::new(),
        }
    }

    pub fn simulate_timeout(&mut self) {
        self.will_timeout = true;
    }

    pub fn get_flashed_firmware(&self) -> Vec<u8> {
        if self.firmware_data_2.is_empty() {
            return self.firmware_data_1.to_owned();
        }

        let mut final_firmware = Vec::new();

        for i in 0..512 {
            let firmware_block_1 = if i * 128 + 128 <= self.firmware_data_1.len() {
                &self.firmware_data_1[i * 128..i * 128 + 128]
            } else {
                &[0u8; 128]
            };
            let firmware_block_2 = if i * 128 + 128 + 64 <= self.firmware_data_2.len() {
                &self.firmware_data_2[(i * 128 + 64)..i * 128 + 128 + 64]
            } else {
                &[0u8; 128]
            };
            final_firmware.extend_from_slice(firmware_block_1);
            final_firmware.extend_from_slice(firmware_block_2);
        }

        final_firmware
    }

    pub fn set_has_rebooted(&mut self, has_rebooted: bool) {
        self.has_rebooted = has_rebooted;
    }

    pub fn get_written_data(&self) -> &Vec<String> {
        &self.written_data
    }
}

#[cfg(test)]
mod tests {
    use bi2a_protocol::{
        command_response::CommandResponse, flash_command::FlashCommand, host_command::HostCommand,
    };

    use super::*;

    fn read_all_bytes(reader: &mut Bi2aTestReader) -> Vec<u8> {
        let mut bytes = Vec::new();
        while let Ok(Some(byte)) = reader.read_byte() {
            bytes.push(byte);
        }
        bytes
    }

    #[test]
    fn test_ping() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&(HostCommand::Ping { ping_value: 0x00 }).to_frame(0x00))
            .unwrap();

        let response = CommandResponse::from_frame(&read_all_bytes(reader), true).unwrap();

        assert_eq!(
            response,
            CommandResponse::Pong {
                sequence_number: 0x00,
                pong_value: 0x01
            }
        );
    }

    #[test]
    fn test_version() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&(HostCommand::Version).to_frame(0x01))
            .unwrap();

        let response = CommandResponse::from_frame(&read_all_bytes(reader), true).unwrap();

        assert_eq!(
            response,
            CommandResponse::Version {
                sequence_number: 0x01,
                major: 0x01,
                minor: 0x02,
                patch: 0x0E,
                name: "BI2A".to_string(),
                date: "Oct 31 2018".to_string(),
                time: "19:11:53".to_string()
            }
        );

        reader.set_has_rebooted(true);

        reader
            .write_bytes(&(HostCommand::Version).to_frame(0x02))
            .unwrap();

        let response = CommandResponse::from_frame(&read_all_bytes(reader), true).unwrap();

        assert_eq!(
            response,
            CommandResponse::Version {
                sequence_number: 0x02,
                major: 0x00,
                minor: 0x00,
                patch: 0x00,
                name: "BI2X".to_string(),
                date: "Apr 23 2019".to_string(),
                time: "10:55:23".to_string()
            }
        );
    }

    #[test]
    fn test_start_flash() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&HostCommand::StartFlash.to_frame(0x03))
            .unwrap();

        let response = CommandResponse::from_frame(&read_all_bytes(reader), true).unwrap();

        assert_eq!(
            response,
            CommandResponse::StartFlash {
                sequence_number: 0x03,
                ok: true
            }
        );
    }

    #[test]
    fn test_are_you_in_flash_mode() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&FlashCommand::AreYouInFlashMode.to_frame())
            .unwrap();

        let bytes = read_all_bytes(reader);

        assert_eq!(
            CommandResponse::from_frame(&bytes[0..2], true),
            Ok(CommandResponse::InFlashingModeOk)
        );
        assert_eq!(
            CommandResponse::from_frame(&bytes[2..], true),
            Ok(CommandResponse::ReadyForNextFlashBlock)
        );
    }

    #[test]
    fn test_start_write() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&FlashCommand::StartWrite { offset: 0x12345678 }.to_frame())
            .unwrap();

        let bytes = read_all_bytes(reader);
        let response = CommandResponse::from_frame(&bytes[..(bytes.len() - 1)], true).unwrap();

        assert_eq!(
            response,
            CommandResponse::StartWriteOk { offset: 0x12345678 }
        );

        let response = CommandResponse::from_frame(&bytes[(bytes.len() - 1)..], true).unwrap();
        assert_eq!(response, CommandResponse::ReadyForNextFlashBlock);
    }

    #[test]
    fn test_write_block() {
        let reader = &mut Bi2aTestReader::new();

        // 64kb firmware
        let firmware_64kb = std::fs::read("assets/test_64k.bin").unwrap();

        for i in (0..512).rev() {
            let firmware_offset = i * 128u32;
            let block_data: [u8; 128] = firmware_64kb
                [firmware_offset as usize..(firmware_offset as usize + 128)]
                .try_into()
                .unwrap();
            let command = FlashCommand::WriteBlock {
                firmware_offset,
                is_128kb_firmware: false,
                block_data,
            };
            reader.write_bytes(&command.to_frame()).unwrap();

            let bytes = read_all_bytes(reader);
            let response = CommandResponse::from_frame(&bytes[..(bytes.len() - 1)], true).unwrap();
            assert_eq!(
                response,
                CommandResponse::WriteBlockOk {
                    firmware_offset: firmware_offset as u16,
                    block_data
                }
            );

            let response = CommandResponse::from_frame(&bytes[(bytes.len() - 1)..], true).unwrap();
            assert_eq!(response, CommandResponse::ReadyForNextFlashBlock);
        }

        let flash_firmware = reader.get_flashed_firmware();
        assert_eq!(flash_firmware, firmware_64kb);

        // 128kb firmware
        let firmware_128kb = std::fs::read("assets/test_128k.bin").unwrap();

        for i in (0..1024).rev() {
            let firmware_offset = i * 128u32;
            let block_data: [u8; 128] = firmware_128kb
                [firmware_offset as usize..(firmware_offset as usize + 128)]
                .try_into()
                .unwrap();
            let command = FlashCommand::WriteBlock {
                firmware_offset,
                is_128kb_firmware: true,
                block_data,
            };
            reader.write_bytes(&command.to_frame()).unwrap();

            let bytes = read_all_bytes(reader);
            let response = CommandResponse::from_frame(&bytes[..(bytes.len() - 1)], true).unwrap();
            assert_eq!(
                response,
                CommandResponse::WriteBlockOk {
                    firmware_offset: (firmware_offset >> 1) as u16,
                    block_data
                }
            );

            let response = CommandResponse::from_frame(&bytes[(bytes.len() - 1)..], true).unwrap();
            assert_eq!(response, CommandResponse::ReadyForNextFlashBlock);
        }

        let flash_firmware = reader.get_flashed_firmware();
        assert_eq!(flash_firmware, firmware_128kb);
    }

    #[test]
    fn test_end_write() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&FlashCommand::EndWrite.to_frame())
            .unwrap();

        let bytes = &read_all_bytes(reader);

        let response = CommandResponse::from_frame(&bytes[0..2], true).unwrap();
        assert_eq!(response, CommandResponse::EndWriteOk);

        assert_eq!(
            CommandResponse::from_frame(&bytes[2..], true),
            Ok(CommandResponse::ReadyForNextFlashBlock)
        );
    }

    #[test]
    fn test_reboot() {
        let reader = &mut Bi2aTestReader::new();

        reader
            .write_bytes(&FlashCommand::Reboot.to_frame())
            .unwrap();

        assert_eq!(
            reader.read_byte(),
            Err(Bio2ReaderError::Raw("Simulated timeout".to_string()))
        );
    }

    #[test]
    fn test_timeout() {
        let reader = &mut Bi2aTestReader::new();

        reader.simulate_timeout();

        assert_eq!(
            reader.read_byte(),
            Err(Bio2ReaderError::Raw("Simulated timeout".to_string()))
        );

        // After the timeout, it should work normally
        reader
            .write_bytes(&HostCommand::Ping { ping_value: 0x42 }.to_frame(0x00))
            .unwrap();
        let response = CommandResponse::from_frame(&read_all_bytes(reader), true).unwrap();
        assert_eq!(
            response,
            CommandResponse::Pong {
                sequence_number: 0x00,
                pong_value: 0x43
            }
        );
    }
}
