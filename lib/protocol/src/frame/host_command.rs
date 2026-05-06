#[derive(Debug, PartialEq, Eq)]
pub enum HostCommand {
    /// Ping the device. The pong value should be the ping value + 1
    Ping { ping_value: u8 },
    /// Get the device version
    Version,
    /// Enter flash mode
    /// You should only use flash commands afterwards
    StartFlash,
    /// Custom command with custom payload
    Custom {
        command_code: u16,
        addr: u8,
        payload: Vec<u8>,
    },
}

impl HostCommand {
    fn command_code(&self) -> u16 {
        match self {
            HostCommand::Ping { .. } => 0x0001,
            HostCommand::Version => 0x0002,
            HostCommand::StartFlash => 0x0004,
            HostCommand::Custom { command_code, .. } => *command_code,
        }
    }

    fn payload(&self) -> Vec<u8> {
        match self {
            HostCommand::Ping { ping_value } => vec![*ping_value],
            HostCommand::Version => vec![],
            HostCommand::StartFlash => vec![],
            HostCommand::Custom {
                addr: _, payload, ..
            } => payload.clone(),
        }
    }

    fn addr(&self) -> u8 {
        match self {
            HostCommand::Ping { .. } => 0x00,
            HostCommand::Version => 0x01,
            HostCommand::StartFlash => 0x01,
            HostCommand::Custom { addr, .. } => *addr,
        }
    }

    pub fn to_frame(&self, sequence_number: u8) -> Vec<u8> {
        let mut res = Vec::new();

        // Destination node address
        res.push(self.addr());

        // Command code
        let command_code = self.command_code();
        res.push((command_code >> 8) as u8); // Command code high byte
        res.push((command_code & 0xFF) as u8); // Command code low byte

        // Sequence number
        res.push(sequence_number);

        // Payload
        let payload = self.payload();
        res.push(payload.len() as u8); // Payload length
        res.extend_from_slice(&payload); // Payload data

        // Checksum (simple sum of all bytes modulo 256)
        let checksum: u8 = res.iter().fold(0u8, |acc, &byte| acc.wrapping_add(byte));
        res.push(checksum);

        // Start delimiter
        let mut escaped_res = Vec::new();
        escaped_res.push(0xAA);

        // Escape bytes
        // 0xAA => 0xFF 0x55
        // 0xFF => 0xFF 0x00
        for &byte in &res {
            match byte {
                0xAA => {
                    escaped_res.push(0xFF);
                    escaped_res.push(0x55);
                }
                0xFF => {
                    escaped_res.push(0xFF);
                    escaped_res.push(0x00);
                }
                _ => escaped_res.push(byte),
            }
        }

        escaped_res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_command() {
        let command = HostCommand::Ping { ping_value: 0x05 };
        let frame = command.to_frame(0x53);
        let expected_frame = vec![0xAA, 0x00, 0x00, 0x01, 0x53, 0x01, 0x05, 0x5A];
        assert_eq!(
            frame, expected_frame,
            "Ping command frame does not match expected value"
        );
    }

    #[test]
    fn test_version_command() {
        let command = HostCommand::Version;
        let frame = command.to_frame(0x19);
        let expected_frame = vec![0xAA, 0x01, 0x00, 0x02, 0x19, 0x00, 0x1C];
        assert_eq!(
            frame, expected_frame,
            "Version command frame does not match expected value"
        );
    }

    #[test]
    fn test_start_flash_command() {
        let command = HostCommand::StartFlash;
        let frame = command.to_frame(0xA5);
        let expected_frame = vec![0xAA, 0x01, 0x00, 0x04, 0xA5, 0x00, 0xFF, 0x55];
        assert_eq!(
            frame, expected_frame,
            "StartFlash command frame does not match expected value"
        );
    }

    #[test]
    fn test_escape_bytes() {
        let command = HostCommand::Ping { ping_value: 0xFF };
        let frame = command.to_frame(0xAA);
        let expected_frame = vec![0xAA, 0x00, 0x00, 0x01, 0xFF, 0x55, 0x01, 0xFF, 0x00, 0xAB];
        assert_eq!(
            frame, expected_frame,
            "Frame with escape byte does not match expected value"
        );
    }
}
