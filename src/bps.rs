use std::io::Write;

use num_enum::TryFromPrimitive;

use crate::{
    bps_ups::{self, ReadVarExt, WriteVarExt},
    Error, Patch, ReadExt, Result,
};

#[repr(u8)]
#[derive(Debug, Clone, Copy, TryFromPrimitive)]
enum Action {
    SourceRead,
    TargetRead,
    SourceCopy,
    TargetCopy,
}

impl From<&Record> for Action {
    fn from(value: &Record) -> Self {
        match value {
            Record::SourceRead => Action::SourceRead,
            Record::TargetRead { .. } => Action::TargetRead,
            Record::SourceCopy { .. } => Action::SourceCopy,
            Record::TargetCopy { .. } => Action::TargetCopy,
        }
    }
}

enum Record {
    SourceRead,
    TargetRead(Vec<u8>),
    SourceCopy(isize),
    TargetCopy(isize),
}

pub struct BpsPatch {
    src_data: bps_ups::Validation,
    out_data: bps_ups::Validation,
    metadata: Option<Vec<u8>>,
    records: Vec<(usize, Record)>,
}

impl BpsPatch {
    const MAGIC: &[u8; 4] = b"BPS1";

    pub fn load(mut data: &[u8]) -> Result<Self> {
        if data.read_arr()? != *Self::MAGIC {
            return Err(Error::Magic(std::str::from_utf8(Self::MAGIC).unwrap()));
        }

        let src_size = data.read_var_int()?;
        let out_size = data.read_var_int()?;
        let metadata = match data.read_var_int()? {
            0 => None,
            len => Some(data.read_vec(len)?),
        };

        let mut records = Vec::new();
        while data.len() > 12 {
            let action = data.read_var_int()?;
            let length = (action >> 2) + 1;
            records.push(match Action::try_from((action & 0b11) as u8) {
                Ok(Action::SourceRead) => (length, Record::SourceRead),
                Ok(Action::TargetRead) => (length, Record::TargetRead(data.read_vec(length)?)),
                Ok(Action::SourceCopy) => {
                    let num = data.read_var_int()?;
                    (
                        length,
                        Record::SourceCopy(
                            if num & 0b1 != 0 { -1 } else { 1 } * (num >> 1) as isize,
                        ),
                    )
                }
                Ok(Action::TargetCopy) => {
                    let num = data.read_var_int()?;
                    (
                        length,
                        Record::TargetCopy(
                            if num & 0b1 != 0 { -1 } else { 1 } * (num >> 1) as isize,
                        ),
                    )
                }
                Err(_) => {
                    todo!();
                }
            });
        }

        let this = Self {
            src_data: bps_ups::Validation {
                size: src_size,
                crc: u32::from_le_bytes(data.read_arr()?),
            },
            out_data: bps_ups::Validation {
                size: out_size,
                crc: u32::from_le_bytes(data.read_arr()?),
            },
            metadata,
            records,
        };

        this.export(Some(u32::from_le_bytes(data.read_arr()?)))?;
        Ok(this)
    }
}

impl Patch for BpsPatch {
    fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
        self.validate(rom).unwrap()?;

        let mut buf = Vec::with_capacity(self.out_data.size);
        let mut src_offset = 0;
        let mut out_offset = 0;
        for (length, record) in self.records.iter() {
            match record {
                Record::SourceRead => {
                    buf.write_all(&rom[buf.len()..][..*length])?;
                }
                Record::TargetRead(data) => {
                    buf.write_all(data)?;
                }
                &Record::SourceCopy(offset) => {
                    src_offset = if offset.is_negative() {
                        src_offset - offset.unsigned_abs()
                    } else {
                        src_offset + offset.unsigned_abs()
                    };
                    buf.write_all(&rom[src_offset..][..*length])?;
                    src_offset += length;
                }
                &Record::TargetCopy(offset) => {
                    out_offset = if offset.is_negative() {
                        out_offset - offset.unsigned_abs()
                    } else {
                        out_offset + offset.unsigned_abs()
                    };
                    // we cant use copy_from_slice or extend because we have to be able to read from
                    // the data as we write it
                    for _ in 0..*length {
                        buf.push(buf[out_offset]);
                        out_offset += 1;
                    }
                }
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

        if let Some(metadata) = &self.metadata {
            buf.write_var_int(metadata.len())?;
            buf.write_all(metadata)?;
        } else {
            buf.write_var_int(0)?;
        }

        for (length, record) in self.records.iter() {
            buf.write_var_int(((*length - 1) << 2) + Action::from(record) as usize)?;
            match record {
                Record::SourceRead => {}
                Record::TargetRead(data) => buf.write_all(data)?,
                Record::SourceCopy(offset) | Record::TargetCopy(offset) => {
                    buf.write_var_int(
                        (offset.unsigned_abs() << 1) | (offset.is_negative() as usize),
                    )?;
                }
            }
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