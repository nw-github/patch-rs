use std::io::{Read, Write};

use anyhow::{bail, Result};

use crate::ReadExt;

enum Record {
    Bytes(Vec<u8>),
    ByteRun(u8, u16),
}

pub struct Patch {
    records: Vec<(usize, Record)>,
    outsz: Option<usize>,
}

impl Patch {
    const MAGIC: &[u8; 5] = b"PATCH";

    pub fn load(mut data: &[u8]) -> Result<Self> {
        if data.read_arr()? != *Self::MAGIC {
            bail!(
                "Patch file is missing the '{}' magic value.",
                std::str::from_utf8(Self::MAGIC).unwrap()
            );
        }

        let mut records = Vec::new();
        while !data.is_empty() {
            let offset = Self::read_u24(&mut data)?;
            if offset == u32::from_be_bytes(*b"\0EOF") {
                if data.len() == 3 {
                    return Ok(Self {
                        records,
                        outsz: Some(Self::read_u24(&mut data)? as usize),
                    });
                }

                break;
            }

            let len = u16::from_be_bytes(data.read_arr()?);
            if len == 0 {
                let len = u16::from_be_bytes(data.read_arr()?);
                records.push((offset as usize, Record::ByteRun(data.read_u8()?, len)));
            } else {
                let mut buf = vec![0; len as usize];
                data.read_exact(&mut buf[..])?;
                records.push((offset as usize, Record::Bytes(buf)));
            }
        }

        Ok(Self {
            records,
            outsz: None
        })
    }

    pub fn apply(&self, rom: &[u8]) -> Result<Vec<u8>> {
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

    pub fn save(&self) -> std::io::Result<Vec<u8>> {
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
            Self::write_u24(&mut buf, *offset as u32)?;
            match record {
                Record::Bytes(data) => {
                    buf.write_all(&(data.len() as u16).to_be_bytes())?;
                    buf.write_all(data)?;
                }
                Record::ByteRun(byte, len) => {
                    buf.write_all(&0u16.to_be_bytes())?;
                    buf.write_all(&len.to_be_bytes())?;
                    buf.write_all(&[*byte])?;
                }
            }
        }

        buf.write_all(b"EOF")?;
        if let Some(outsz) = self.outsz {
            Self::write_u24(&mut buf, outsz as u32)?;
        }

        Ok(buf)
    }

    fn read_u24(data: &mut impl Read) -> std::io::Result<u32> {
        let mut buf = [0; 4];
        data.read_exact(&mut buf[1..])?;
        Ok(u32::from_be_bytes(buf))
    }

    fn write_u24(data: &mut impl Write, val: u32) -> std::io::Result<()> {
        data.write_all(&val.to_be_bytes()[1..])
    }
}
