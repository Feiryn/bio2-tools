use std::any::Any;

use thiserror::Error;

pub trait Bio2Reader {
    fn read_byte(&mut self) -> Result<Option<u8>, Bio2ReaderError>;
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), Bio2ReaderError>;
    fn reconnect(&mut self) -> Result<(), Bio2ReaderError>;
    fn clear(&mut self) -> ();
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Bio2ReaderError {
    #[error("Reader error: {0}")]
    Raw(String),
}
