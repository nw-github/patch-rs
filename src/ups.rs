use byteorder::{ReadBytesExt, WriteBytesExt, LE};

use crate::{
    bps_ups::{self, ReadVarExt, WriteVarExt},
    Error, Patch, ReadExt, Result,
};
use std::{
    io::{BufRead, Write},
    iter,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UpsPatch {
    src_data: bps_ups::Validation,
    out_data: bps_ups::Validation,
    records: Vec<(usize, Vec<u8>)>,
}

impl UpsPatch {
    const MAGIC: &[u8; 4] = b"UPS1";

    pub fn load(mut patch: &[u8]) -> Result<Self> {
        if patch.read_arr()? != *Self::MAGIC {
            return Err(Error::Magic(unsafe {
                std::str::from_utf8_unchecked(Self::MAGIC)
            }));
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
                crc: patch.read_u32::<LE>()?,
            },
            out_data: bps_ups::Validation {
                size: new_size,
                crc: patch.read_u32::<LE>()?,
            },
            records,
        };

        result.export(Some(patch.read_u32::<LE>()?))?;
        Ok(result)
    }

    pub fn create(src: &[u8], dst: &[u8]) -> Self {
        let mut records = Vec::new();
        let mut iter = src
            .iter()
            .chain(iter::repeat(&0).take(dst.len().saturating_sub(src.len())))
            .zip(dst.iter())
            .enumerate();
        while let Some((i, (s, d))) = iter.next() {
            if s != d {
                records.push((
                    i,
                    iter::once((s, d))
                        .chain(
                            iter.by_ref()
                                .take_while(|(_, (s, d))| s != d)
                                .map(|(_, bytes)| bytes),
                        )
                        .map(|(s, d)| s ^ d)
                        .chain(iter::once(0))
                        .collect(),
                ));
            }
        }

        Self {
            src_data: bps_ups::Validation {
                size: src.len(),
                crc: crc32fast::hash(src),
            },
            out_data: bps_ups::Validation {
                size: dst.len(),
                crc: crc32fast::hash(dst),
            },
            records,
        }
    }
}

impl Patch for UpsPatch {
    fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
        self.validate(rom).unwrap()?;

        let mut buf = vec![0; self.out_data.size];
        let size = rom.len().min(buf.len());
        buf[..size].copy_from_slice(&rom[..size]);

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

        for (i, record) in self.records.iter().enumerate() {
            buf.write_var_int(if i > 0 {
                record.0 - (self.records[i - 1].0 + self.records[i - 1].1.len())
            } else {
                record.0
            })?;
            buf.write_all(&record.1)?;
        }

        buf.write_u32::<LE>(self.src_data.crc)?;
        buf.write_u32::<LE>(self.out_data.crc)?;

        let hash = crc32fast::hash(&buf);
        if let Some(crc) = crc {
            if hash != crc {
                return Err(Error::InvalidCRC(hash, crc));
            }
        }

        buf.write_u32::<LE>(hash)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_same_len() {
        let src = b"It's better to be happy than to be right.";
        let dst = b"It's better to be right than to be happy.";

        let patch = UpsPatch::create(src, dst);
        assert_eq!(&patch.apply(src).unwrap(), dst);
    }

    #[test]
    fn patch_shorter_src() {
        let src = b"/bin/true";
        let dst = b"/usr/bin/sh";

        let patch = UpsPatch::create(src, dst);
        assert_eq!(&patch.apply(src).unwrap(), dst);
    }

    #[test]
    fn patch_shorter_dst() {
        let src = b"The source is longer.";
        let dst = b"The dest is shorter.";

        let patch = UpsPatch::create(src, dst);
        assert_eq!(&patch.apply(src).unwrap(), dst);
    }
}
