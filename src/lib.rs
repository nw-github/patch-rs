use std::{
    io::{self, Read},
    mem::{self, MaybeUninit},
    slice,
};

use thiserror::Error;

mod bps;
mod ips;
mod ups;

pub mod prelude {
    pub use super::bps::BpsPatch;
    pub use super::ips::IpsPatch;
    pub use super::ups::UpsPatch;
    pub use super::Patch;
}

pub(crate) trait ReadExt: Read {
    #[inline]
    fn read_arr<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        unsafe {
            let mut buf: [MaybeUninit<u8>; N] = MaybeUninit::uninit().assume_init();
            self.read_exact(mem::transmute(buf.as_mut_slice()))?;
            // we'd like to use `Ok(std::mem::transmute(buf))`
            // but as of rust 1.69 this won't compile with N as a generic const
            Ok(*(&buf as *const _ as *const _))
        }
    }

    #[inline]
    fn read_vec(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(len);
        unsafe {
            self.read_exact(slice::from_raw_parts_mut(buf.as_mut_ptr(), len))?;
            buf.set_len(len);
        }
        Ok(buf)
    }
}

impl<T: Read + ?Sized> ReadExt for T {}

#[derive(Error, Debug)]
pub enum Error {
    #[error("File is missing magic value '{0}'.")]
    Magic(&'static str),
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Size ({0:#X} bytes) does not match expected value ({1:#X} bytes).")]
    InvalidSize(usize, usize),
    #[error("CRC ({0:#X}) does not match expected value ({1:#X}).")]
    InvalidCRC(u32, u32),
    #[error("The patch is invalid.")]
    InvalidPatch,
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

pub(crate) mod bps_ups {
    use std::io::{Read, Write};

    use byteorder::{ReadBytesExt, WriteBytesExt};

    use crate::{Error, Result};

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct Validation {
        pub size: usize,
        pub crc: u32,
    }

    impl Validation {
        pub fn validate(&self, data: &[u8]) -> Result<()> {
            if self.size != data.len() {
                return Err(Error::InvalidSize(data.len(), self.size));
            }

            let hash = crc32fast::hash(data);
            if hash != self.crc {
                return Err(Error::InvalidCRC(hash, self.crc));
            }

            Ok(())
        }
    }

    pub trait ReadVarExt: Read {
        fn read_var_int(&mut self) -> std::io::Result<usize> {
            let mut value = 0;
            let mut shift = 1;
            loop {
                let x = self.read_u8()?;
                value += (x as usize & 0x7f) * shift;
                if (x & 0x80) != 0 {
                    return Ok(value);
                }

                shift <<= 7;
                value += shift;
            }
        }
    }

    impl<T: Read + ?Sized> ReadVarExt for T {}

    pub trait WriteVarExt: Write {
        fn write_var_int(&mut self, mut value: usize) -> std::io::Result<()> {
            loop {
                let x = (value & 0x7f) as u8;
                value >>= 7;
                if value == 0 {
                    self.write_u8(0x80 | x)?;
                    return Ok(());
                }

                self.write_u8(x)?;
                value -= 1;
            }
        }
    }

    impl<T: Write + ?Sized> WriteVarExt for T {}
}
