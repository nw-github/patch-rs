use std::io::{self, Read};

use thiserror::Error;

mod ips;
mod ups;

pub mod prelude {
    pub use super::ips::IpsPatch;
    pub use super::ups::UpsPatch;
    pub use super::Patch;
}

pub(crate) trait ReadExt: Read {
    fn read_arr<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        Ok(self.read_arr::<1>()?[0])
    }
}

impl<T: Read + ?Sized> ReadExt for T {}

#[derive(Error, Debug)]
pub enum Error {
    #[error("File is missing magic value '{0}'.")]
    Magic(&'static str),
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Size of {0} ({1:#X} bytes) does not match expected value ({2:#X} bytes).")]
    InvalidSize(&'static str, usize, usize),
    #[error("CRC of {0} ({1:#X}) does not match expected value ({2:#X}).")]
    InvalidCRC(&'static str, u32, u32),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Patch {
    fn apply(&self, rom: &[u8]) -> Result<Vec<u8>>;
    fn validate(&self, rom: &[u8]) -> Option<Result<()>>;
    fn export(&self, crc: Option<u32>) -> Result<Vec<u8>>;
}
