use std::io;
use std::io::{Read, Write};

pub trait ReadExt: Read {
    fn read_cstring(&mut self) -> io::Result<String>;
}

impl<T: Read> ReadExt for T {
    fn read_cstring(&mut self) -> io::Result<String> {
        let mut bytes: Vec<u8> = Vec::new();
        for byte in self.bytes() {
            let b = byte?;
            if b == 0 {
                break;
            } else {
                bytes.push(b);
            }

        }

        Ok(String::from_utf8(bytes).unwrap())
    }
}

pub trait WriteExt: Write {
    fn write_cstring<S: AsRef<[u8]>>(&mut self, s: S) -> io::Result<()>;
}

impl<T: Write> WriteExt for T {
    fn write_cstring<S: AsRef<[u8]>>(&mut self, s: S) -> io::Result<()> {
        self.write_all(s.as_ref())?;
        self.write_all(b"\0")?;
        Ok(())
    }
}
