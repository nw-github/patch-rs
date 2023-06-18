use std::io::{self, Read};

pub mod ups;

pub(crate) trait ReadHelper {
    fn readn<const N: usize>(&mut self) -> io::Result<[u8; N]>;

    fn read_u8(&mut self) -> io::Result<u8> {
        Ok(self.readn::<1>()?[0])
    }
}

impl<T: Read> ReadHelper for T {
    fn readn<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;

        Ok(buf)
    }
}
