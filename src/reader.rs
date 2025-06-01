use crate::errors::{ParseError, Result};
use crate::unreal_types::FName;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom};

pub trait UassetReader {
    fn read_fname(&mut self) -> Result<FName>;
    fn read_fstring(&mut self) -> Result<String>;
    fn skip_bytes(&mut self, n: i64) -> Result<()>;
    fn read_tarray<T, F>(&mut self, reader_fn: F, max_elements: usize) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>;
}

impl<R: Read + Seek> UassetReader for R {
    fn read_fname(&mut self) -> Result<FName> {
        let index = self.read_i32::<LittleEndian>()?;
        let number = self.read_i32::<LittleEndian>()?;
        Ok(FName { index, number })
    }

    fn read_fstring(&mut self) -> Result<String> {
        let size = self.read_i32::<LittleEndian>()?;

        if size == 0 {
            return Ok(String::new());
        }

        let (load_ucs2_char, actual_size) = if size < 0 {
            (true, (-size) as usize)
        } else {
            (false, size as usize)
        };

        let byte_size = if load_ucs2_char {
            actual_size * 2
        } else {
            actual_size
        };

        let mut buffer = vec![0u8; byte_size];
        self.read_exact(&mut buffer)?;

        // Remove null terminator
        if load_ucs2_char {
            buffer.truncate(byte_size - 2);
            // Convert UTF-16LE to String
            let u16_vec: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16(&u16_vec).map_err(|_| ParseError::InvalidUtf16)
        } else {
            buffer.truncate(byte_size - 1);
            String::from_utf8(buffer).map_err(|e| e.into())
        }
    }

    fn skip_bytes(&mut self, n: i64) -> Result<()> {
        self.seek(SeekFrom::Current(n))?;
        Ok(())
    }

    fn read_tarray<T, F>(&mut self, mut reader_fn: F, max_elements: usize) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>,
    {
        let n = self.read_i32::<LittleEndian>()?;

        if n < 0 || n as usize > max_elements {
            return Err(ParseError::InvalidArraySize(n));
        }

        let mut array = Vec::with_capacity(n as usize);
        for _ in 0..n {
            array.push(reader_fn(self)?);
        }
        Ok(array)
    }
}
