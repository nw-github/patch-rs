use crate::ReadExt;
use anyhow::{bail, Result};
use std::io::{BufRead, Read, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PatchError {
    #[error("Size of {0} ({1:#X} bytes) does not match expected value ({2:#X} bytes).")]
    InvalidSize(&'static str, usize, usize),
    #[error("CRC of {0} ({1:#X}) does not match expected value ({2:#X}).")]
    InvalidCRC(&'static str, u32, u32),
}

pub struct Patch {
    old_size: u64,
    new_size: u64,
    old_crc: u32,
    new_crc: u32,
    records: Vec<(usize, Vec<u8>)>,
}

impl Patch {
    const MAGIC: &[u8; 4] = b"UPS1";

    pub fn load(mut patch: &[u8]) -> Result<Self> {
        if patch.read_arr()? != *Self::MAGIC {
            bail!(
                "Patch file is missing the '{}' magic value.",
                std::str::from_utf8(Self::MAGIC).unwrap()
            );
        }

        let old_size = Self::decode_var_int(&mut patch)?;
        let new_size = Self::decode_var_int(&mut patch)?;

        let mut records = Vec::new();
        let mut fpos = 0;
        while patch.len() > 12 {
            fpos += Self::decode_var_int(&mut patch)? as usize;
            records.push((fpos, {
                let mut buf = Vec::new();
                fpos += patch.read_until(0, &mut buf)?;
                buf
            }));
        }

        let result = Self {
            old_size,
            new_size,
            old_crc: u32::from_le_bytes(patch.read_arr()?),
            new_crc: u32::from_le_bytes(patch.read_arr()?),
            records,
        };

        result.save(u32::from_le_bytes(patch.read_arr()?))?;
        Ok(result)
    }

    pub fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
        if self.old_size as usize != rom.len() {
            return Err(
                PatchError::InvalidSize("ROM file", rom.len(), self.old_size as usize).into(),
            );
        }

        let hash = crc32fast::hash(&rom);
        if hash != self.old_crc {
            return Err(PatchError::InvalidCRC("ROM", hash, self.old_crc).into());
        }

        let mut buf = Vec::from(rom);
        if self.new_size as usize > buf.len() {
            buf.resize(self.new_size as usize, 0);
        }

        for (offset, xor_bytes) in self.records.iter() {
            for u in 0..xor_bytes.len() - 1 {
                buf[*offset + u] ^= xor_bytes[u];
            }
        }

        let hash = crc32fast::hash(&buf);
        if hash != self.new_crc {
            return Err(PatchError::InvalidCRC("patched ROM", hash, self.new_crc).into());
        }

        Ok(buf)
    }

    fn save(&self, expected_crc: u32) -> Result<Vec<u8>> {
        let mut buf = Vec::new();

        buf.write_all(Self::MAGIC)?;
        Self::encode_var_int(&mut buf, self.old_size)?;
        Self::encode_var_int(&mut buf, self.new_size)?;

        for i in 0..self.records.len() {
            let mut relative = self.records[i].0;
            if i > 0 {
                relative -= self.records[i - 1].0 + self.records[i - 1].1.len();
            }

            Self::encode_var_int(&mut buf, relative as u64)?;
            buf.write_all(&self.records[i].1)?;
        }

        buf.write_all(&self.old_crc.to_le_bytes())?;
        buf.write_all(&self.new_crc.to_le_bytes())?;

        let hash = crc32fast::hash(&buf);
        if hash != expected_crc {
            return Err(PatchError::InvalidCRC("output file", hash, expected_crc).into());
        }

        buf.write_all(&hash.to_le_bytes())?;
        Ok(buf)
    }

    fn decode_var_int(data: &mut impl Read) -> std::io::Result<u64> {
        let mut value = 0;
        let mut shift = 1;
        loop {
            let x = data.read_u8()?;
            value += (x as u64 & 0x7f) * shift;
            if (x & 0x80) != 0 {
                return Ok(value);
            }

            shift <<= 7;
            value += shift;
        }
    }

    fn encode_var_int(out: &mut impl Write, mut value: u64) -> std::io::Result<()> {
        loop {
            let x = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.write_all(&[0x80 | x])?;
                return Ok(());
            }

            out.write_all(&[x])?;
            value -= 1;
        }
    }
}
