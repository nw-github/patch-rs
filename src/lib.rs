use std::io::{self, Read};

pub mod ups;

pub(crate) trait ReadExt : Read {
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
