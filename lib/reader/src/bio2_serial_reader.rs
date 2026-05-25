use std::{any::Any, time::Duration};

use serial::SerialPort;
use thiserror::Error;

use crate::bio2_reader::{Bio2Reader, Bio2ReaderError};

pub struct Bio2SerialReader {
    firmware: Bio2SerialFirmware,
    com_port: String,
    timeout_ms: Option<u64>,
    serial_port: Box<dyn SerialPort>,
    read_buffer: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Bio2SerialFirmware {
    Bi2a,
    Bi2x,
}

impl Bio2SerialReader {
    fn open(
        firmware: &Bio2SerialFirmware,
        com_port: &String,
        timeout_ms: Option<u64>,
    ) -> Result<Box<dyn SerialPort>, Bio2SerialReaderError> {
        let mut port =
            serial::open(com_port).map_err(Bio2SerialReaderError::SerialPortOpenError)?;

        port.reconfigure(&|settings| {
            settings
                .set_baud_rate(if *firmware == Bio2SerialFirmware::Bi2a {
                    serial::Baud115200
                } else {
                    serial::Baud9600
                })
                .expect("Failed to set baud rate");
            settings.set_char_size(serial::Bits8);
            settings.set_parity(serial::ParityNone);
            settings.set_stop_bits(serial::Stop1);
            settings.set_flow_control(serial::FlowNone);
            Ok(())
        })
        .map_err(Bio2SerialReaderError::SerialPortConfigError)?;

        port.set_timeout(Duration::from_millis(timeout_ms.unwrap_or(500)))
            .map_err(Bio2SerialReaderError::SerialPortTimeoutError)?;

        Ok(Box::new(port))
    }

    pub fn new(
        firmware: Bio2SerialFirmware,
        com_port: String,
        timeout_ms: Option<u64>,
    ) -> Result<Self, Bio2SerialReaderError> {
        let serial_port = Self::open(&firmware, &com_port, timeout_ms)?;

        Ok(Bio2SerialReader {
            firmware,
            com_port,
            timeout_ms,
            serial_port,
            read_buffer: Vec::new(),
        })
    }

    fn read_from_serial(&mut self) -> Result<(), Bio2SerialReaderError> {
        let mut buf = [0u8; 1024];

        let bytes_read = self
            .serial_port
            .read(&mut buf)
            .map_err(Bio2SerialReaderError::SerialPortReadError)?;

        self.read_buffer.extend_from_slice(&buf[..bytes_read]);

        Ok(())
    }
}

impl Bio2Reader for Bio2SerialReader {
    fn read_byte(&mut self) -> Result<Option<u8>, Bio2ReaderError> {
        if self.read_buffer.is_empty() {
            self.read_from_serial()
                .map_err(|e| Bio2ReaderError::Raw(e.to_string()))?;
        }

        if let Some(byte) = self.read_buffer.first() {
            let byte = *byte;
            self.read_buffer.remove(0);
            Ok(Some(byte))
        } else {
            Ok(None)
        }
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<(), Bio2ReaderError> {
        self.serial_port
            .write(data)
            .map_err(|e| Bio2ReaderError::Raw(e.to_string()))?;
        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), Bio2ReaderError> {
        let port = Self::open(&self.firmware, &self.com_port, self.timeout_ms)
            .map_err(|e| Bio2ReaderError::Raw(e.to_string()))?;
        self.serial_port = port;
        Ok(())
    }

    fn clear(&mut self) -> () {
        self.read_buffer.clear();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Error)]
pub enum Bio2SerialReaderError {
    #[error("Failed to open serial port: {0}")]
    SerialPortOpenError(serial::Error),
    #[error("Failed to configure serial port: {0}")]
    SerialPortConfigError(serial::Error),
    #[error("Failed to set serial port timeout: {0}")]
    SerialPortTimeoutError(serial::Error),
    #[error("Failed to read from serial port: {0}")]
    SerialPortReadError(std::io::Error),
}
