use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt, BE};

use crate::{Error, Patch, ReadExt, Result};

enum Record {
    Bytes(Vec<u8>),
    ByteRun(u8, u16),
}

pub struct IpsPatch {
    records: Vec<(usize, Record)>,
    outsz: Option<usize>,
}

impl IpsPatch {
    const MAGIC: &[u8; 5] = b"PATCH";

    pub fn load(mut data: &[u8]) -> Result<Self> {
        if data.read_arr()? != *Self::MAGIC {
            return Err(Error::Magic(std::str::from_utf8(Self::MAGIC).unwrap()));
        }

        let mut records = Vec::new();
        while !data.is_empty() {
            let offset = data.read_u24::<BE>()?;
            if offset == u32::from_be_bytes(*b"\0EOF") {
                if data.len() == 3 {
                    return Ok(Self {
                        records,
                        outsz: Some(data.read_u24::<BE>()? as usize),
                    });
                }

                break;
            }

            let len = data.read_u16::<BE>()?;
            if len == 0 {
                let len = data.read_u16::<BE>()?;
                records.push((offset as usize, Record::ByteRun(data.read_u8()?, len)));
            } else {
                let mut buf = vec![0; len as usize];
                data.read_exact(&mut buf[..])?;
                records.push((offset as usize, Record::Bytes(buf)));
            }
        }

        Ok(Self {
            records,
            outsz: None,
        })
    }
}

impl Patch for IpsPatch {
    fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![
            0;
            self.outsz.unwrap_or_else(|| {
                self.records
                    .iter()
                    .fold(rom.len(), |acc, (offset, record)| {
                        Some(match record {
                            Record::Bytes(buf) => offset + buf.len(),
                            &Record::ByteRun(_, len) => offset + len as usize,
                        })
                        .filter(|&v| v > acc)
                        .unwrap_or(acc)
                    })
            })
        ];
        let copy = buf.len().min(rom.len());
        buf[..copy].copy_from_slice(&rom[..copy]);

        for (offset, record) in self.records.iter() {
            match record {
                Record::Bytes(bytes) => buf[*offset..][..bytes.len()].copy_from_slice(bytes),
                &Record::ByteRun(byte, len) => buf[*offset..][..len as usize].fill(byte),
            }
        }

        Ok(buf)
    }

    fn validate(&self, _rom: &[u8]) -> Option<Result<()>> {
        None
    }

    fn export(&self, _crc: Option<u32>) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(self.records.iter().fold(
            Self::MAGIC.len(),
            |acc, (_, record)| {
                acc + match record {
                    Record::Bytes(data) => 3 + 2 + data.len(),
                    Record::ByteRun(_, _) => 3 + 2 + 2 + 1,
                }
            },
        ));

        buf.write_all(Self::MAGIC)?;
        for (offset, record) in self.records.iter() {
            buf.write_u24::<BE>(*offset as u32)?;
            match record {
                Record::Bytes(data) => {
                    buf.write_u16::<BE>(data.len() as _)?;
                    buf.write_all(data)?;
                }
                Record::ByteRun(byte, len) => {
                    buf.write_u16::<BE>(0)?;
                    buf.write_u16::<BE>(*len)?;
                    buf.write_u8(*byte)?;
                }
            }
        }

        buf.write_all(b"EOF")?;
        if let Some(outsz) = self.outsz {
            buf.write_u24::<BE>(outsz as _)?;
        }

        Ok(buf)
    }
}
