use crate::{
    bps_ups::{self, ReadVarExt, WriteVarExt},
    Error, Patch, ReadExt, Result,
};
use std::io::{BufRead, Write};

pub struct UpsPatch {
    src_data: bps_ups::Validation,
    out_data: bps_ups::Validation,
    records: Vec<(usize, Vec<u8>)>,
}

impl UpsPatch {
    const MAGIC: &[u8; 4] = b"UPS1";

    pub fn load(mut patch: &[u8]) -> Result<Self> {
        if patch.read_arr()? != *Self::MAGIC {
            return Err(Error::Magic(std::str::from_utf8(Self::MAGIC).unwrap()));
        }

        let old_size = patch.read_var_int()?;
        let new_size = patch.read_var_int()?;

        let mut records = Vec::new();
        let mut fpos = 0;
        while patch.len() > 12 {
            fpos += patch.read_var_int()?;
            records.push((fpos, {
                let mut buf = Vec::new();
                fpos += patch.read_until(0, &mut buf)?;
                buf
            }));
        }

        let result = Self {
            src_data: bps_ups::Validation {
                size: old_size,
                crc: u32::from_le_bytes(patch.read_arr()?),
            },
            out_data: bps_ups::Validation {
                size: new_size,
                crc: u32::from_le_bytes(patch.read_arr()?),
            },
            records,
        };

        result.export(Some(u32::from_le_bytes(patch.read_arr()?)))?;
        Ok(result)
    }
}

impl Patch for UpsPatch {
    fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
        self.validate(rom).unwrap()?;

        let mut buf = Vec::from(rom);
        if self.out_data.size > buf.len() {
            buf.resize(self.out_data.size, 0);
        }

        for (offset, xor_bytes) in self.records.iter() {
            for u in 0..xor_bytes.len() - 1 {
                buf[*offset + u] ^= xor_bytes[u];
            }
        }

        self.out_data.validate(&buf)?;
        Ok(buf)
    }

    fn validate(&self, rom: &[u8]) -> Option<Result<()>> {
        Some(self.src_data.validate(rom))
    }

    fn export(&self, crc: Option<u32>) -> Result<Vec<u8>> {
        let mut buf = Vec::new();

        buf.write_all(Self::MAGIC)?;
        buf.write_var_int(self.src_data.size)?;
        buf.write_var_int(self.out_data.size)?;

        for i in 0..self.records.len() {
            let mut relative = self.records[i].0;
            if i > 0 {
                relative -= self.records[i - 1].0 + self.records[i - 1].1.len();
            }

            buf.write_var_int(relative)?;
            buf.write_all(&self.records[i].1)?;
        }

        buf.write_all(&self.src_data.crc.to_le_bytes())?;
        buf.write_all(&self.out_data.crc.to_le_bytes())?;

        let hash = crc32fast::hash(&buf);
        if let Some(crc) = crc {
            if hash != crc {
                return Err(Error::InvalidCRC(hash, crc));
            }
        }

        buf.write_all(&hash.to_le_bytes())?;
        Ok(buf)
    }
}
