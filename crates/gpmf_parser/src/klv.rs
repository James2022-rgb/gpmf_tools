pub use chrono::NaiveDateTime;

use thiserror::Error;

use std::io::{Read, Seek};

use byteorder::{BigEndian, ReadBytesExt as _};

// https://github.com/gopro/gpmf-parser
// https://exiftool.org/TagNames/GoPro.html

#[derive(Debug, Error)]
pub enum KlvError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unknown value type: \'{}\'/(0x{:02X})", char::from(0), 0)]
    UnknownValueType(u8),
    #[error("FourCC value source is 0x00000000")]
    ZeroFourcc,
}

#[derive(Debug, Clone)]
pub struct Klv {
    header: Header,
    value: Value,
}

impl Klv {
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Vec<Self>, KlvError> {
        let mut klvs: Vec<Self> = Default::default();

        loop {
            let header = Header::from_reader(reader);
            match header {
                Err(KlvError::ZeroFourcc) => {
                    break;
                }
                Err(KlvError::Io(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(err) => return Err(err),
                Ok(header) => {
                    let value = Value::from_reader(reader, header)?;
                    klvs.push(Self { header, value });
                }
            }
        }

        Ok(klvs)
    }

    pub fn header(&self) -> Header {
        self.header
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn into_value(self) -> Value {
        self.value
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Header {
    fourcc: Fourcc,
    tsr: TypeSizeRepeat,
}

impl Header {
    /// Reads exactly 8 bytes.
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, KlvError> {
        let fourcc = Fourcc::from_reader(reader)?;
        let tsr = TypeSizeRepeat::from_reader(reader)?;

        Ok(Self { fourcc, tsr })
    }

    pub fn fourcc(&self) -> Fourcc {
        self.fourcc
    }

    pub fn tsr(&self) -> TypeSizeRepeat {
        self.tsr
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fourcc(pub [u8; 4]);

impl Fourcc {
    /// Reads exactly 4 bytes.
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, KlvError> {
        let mut bytes = [0; 4];
        reader.read_exact(&mut bytes)?;

        if bytes == [0; 4] {
            return Err(KlvError::ZeroFourcc);
        }

        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("Fourcc is not a valid UTF-8 string.")
    }
}

impl std::fmt::Debug for Fourcc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/0x{:04X}",
            std::str::from_utf8(&self.0).unwrap(),
            u32::from_be_bytes(self.0)
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeSizeRepeat {
    /// See https://github.com/gopro/gpmf-parser?tab=readme-ov-file#type
    typ: ValueType,
    /// 8-bits is used for a sample size, each sample is limited to 255 bytes or less.
    sample_size: u8,
    /// 16-bits is used to indicate the number of samples in a GPMF payload, this is the Repeat field.
    /// Struct Size and the Repeat allow for up to 16.7MB of data in a single KLV GPMF payload.
    repeat: u16,
}

impl TypeSizeRepeat {
    /// Reads exactly 4 bytes.
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, KlvError> {
        let type_u8 = reader.read_u8()?;
        let typ = ValueType::try_from(type_u8);
        if typ.is_err() {
            return Err(KlvError::UnknownValueType(type_u8));
        };
        let typ = typ.unwrap();
        let sample_size = reader.read_u8()?;
        let repeat = reader.read_u16::<BigEndian>()?;

        Ok(Self {
            typ,
            sample_size,
            repeat,
        })
    }

    pub fn axis_count(&self) -> usize {
        let single_size = self.typ.element_size();
        if single_size == 0 {
            return 1;
        }
        self.sample_size as usize / single_size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ValueType {
    S8,
    U8,
    S32,
    U32,
    Ascii,
    F32,
    Fourcc,
    U64,
    S16,
    U16,
    DateTime,
    Complex,
    Nested,
}

impl TryFrom<u8> for ValueType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            b'b' => Ok(Self::S8),
            b'B' => Ok(Self::U8),
            b'l' => Ok(Self::S32),
            b'L' => Ok(Self::U32),
            b'c' => Ok(Self::Ascii),
            b'f' => Ok(Self::F32),
            b'F' => Ok(Self::Fourcc),
            b'J' => Ok(Self::U64),
            b's' => Ok(Self::S16),
            b'S' => Ok(Self::U16),
            b'U' => Ok(Self::DateTime),
            b'?' => Ok(Self::Complex),
            b'\0' => Ok(Self::Nested),
            _ => Err(format!(
                "Unknown value type: {}/0x{:02X}",
                char::from(value),
                value
            )),
        }
    }
}

impl ValueType {
    /// Returns `0` for `Complex` and `Nested`.
    pub const fn element_size(&self) -> usize {
        match self {
            Self::S8 => 1,
            Self::U8 => 1,
            Self::S32 => 4,
            Self::U32 => 4,
            Self::Ascii => 1,
            Self::F32 => 4,
            Self::Fourcc => 4,
            Self::U64 => 8,
            Self::S16 => 2,
            Self::U16 => 2,
            Self::DateTime => 16,
            Self::Complex => 0,
            Self::Nested => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    S8(Vec<i8>),
    U8(Vec<u8>),
    S32(Vec<i32>),
    U32(Vec<u32>),
    Ascii(String),
    F32(Vec<f32>),
    Fourcc(Vec<Fourcc>),
    U64(Vec<u64>),
    S16(Vec<i16>),
    U16(Vec<u16>),
    DateTime(NaiveDateTime),
    Complex(ComplexValue),
    Nested(Vec<Klv>),
}

impl Value {
    pub fn from_reader<R: Read + Seek>(reader: &mut R, header: Header) -> Result<Self, KlvError> {
        match header.tsr.typ {
            ValueType::S8 => Ok(Self::S8(Self::read_numeric(reader, header)?)),
            ValueType::U8 => Ok(Self::U8(Self::read_numeric(reader, header)?)),
            ValueType::S32 => Ok(Self::S32(Self::read_numeric(reader, header)?)),
            ValueType::U32 => Ok(Self::U32(Self::read_numeric(reader, header)?)),
            ValueType::Ascii => {
                let mut bytes =
                    vec![0; header.tsr.sample_size as usize * header.tsr.repeat as usize];
                reader.read_exact(&mut bytes)?;
                Self::skip_padding(reader, bytes.len())?;

                /// Converts from Latin1(ISO-8859-1) to UTF-8.
                fn latin1_to_utf8(bytes: &[u8]) -> String {
                    // ISO-8859-1 is a subset of Unicode codepoints.
                    bytes.iter().map(|&b| b as char).collect()
                }

                Ok(Self::Ascii(latin1_to_utf8(&bytes)))
            }
            ValueType::F32 => Ok(Self::F32(Self::read_numeric(reader, header)?)),
            ValueType::Fourcc => {
                let axis_count = header.tsr.axis_count();
                let value_count = axis_count * (header.tsr.repeat as usize);
                let values: Vec<Fourcc> = (0..value_count)
                    .map(|_| Fourcc::from_reader(reader).unwrap())
                    .collect();
                Ok(Self::Fourcc(values))
            }
            ValueType::U64 => Ok(Self::U64(Self::read_numeric(reader, header)?)),
            ValueType::S16 => Ok(Self::S16(Self::read_numeric(reader, header)?)),
            ValueType::U16 => Ok(Self::U16(Self::read_numeric(reader, header)?)),
            ValueType::DateTime => {
                let mut bytes =
                    vec![0; header.tsr.sample_size as usize * header.tsr.repeat as usize];
                reader.read_exact(&mut bytes)?;
                Self::skip_padding(reader, bytes.len())?;

                /// Converts from Latin1(ISO-8859-1) to UTF-8.
                fn latin1_to_utf8(bytes: &[u8]) -> String {
                    // ISO-8859-1 is a subset of Unicode codepoints.
                    bytes.iter().map(|&b| b as char).collect()
                }

                let string = latin1_to_utf8(&bytes);
                let date_time = NaiveDateTime::parse_from_str(&string, "%y%m%d%H%M%S%.f").unwrap();

                Ok(Self::DateTime(date_time))
            }
            ValueType::Complex => {
                let mut bytes =
                    vec![0; header.tsr.sample_size as usize * header.tsr.repeat as usize];
                reader.read_exact(&mut bytes)?;
                Self::skip_padding(reader, bytes.len())?;

                Ok(Self::Complex(ComplexValue { raw_data: bytes }))
            }
            ValueType::Nested => {
                let mut klvs: Vec<Klv> = Vec::new();

                let mut position = reader.stream_position()?;
                let end_position = position
                    + (header.tsr.sample_size as u16 * header.tsr.repeat).next_multiple_of(4)
                        as u64;
                while position < end_position {
                    let header = Header::from_reader(reader)?;
                    let value = Value::from_reader(reader, header)?;
                    klvs.push(Klv { header, value });

                    position = reader.stream_position()?;
                }

                Ok(Self::Nested(klvs))
            }
        }
    }

    fn read_numeric<T: Numeric + std::fmt::Debug, R: Read>(
        reader: &mut R,
        header: Header,
    ) -> Result<Vec<T>, std::io::Error> {
        let axis_count = header.tsr.axis_count();
        let value_count = axis_count * (header.tsr.repeat as usize);

        let mut values: Vec<T> = Vec::with_capacity(value_count);
        T::values_from_reader(reader, unsafe {
            std::slice::from_raw_parts_mut(values.as_mut_ptr(), value_count)
        })?;
        unsafe {
            values.set_len(value_count);
        }

        Self::skip_padding(reader, std::mem::size_of_val(values.as_slice()))?;
        Ok(values)
    }

    fn skip_padding<R: Read>(reader: &mut R, bytes_processed: usize) -> Result<(), std::io::Error> {
        let padding_size = bytes_processed.next_multiple_of(4) - bytes_processed;

        let mut max_padding: [u8; 4] = [0; 4];
        reader.read_exact(&mut max_padding[0..padding_size])?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ComplexValue {
    raw_data: Vec<u8>,
}

impl ComplexValue {
    pub fn raw_data(&self) -> &[u8] {
        &self.raw_data
    }
}

trait Numeric {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error>
    where
        Self: Sized;

    fn values_from_reader<R: Read>(reader: &mut R, dst: &mut [Self]) -> Result<(), std::io::Error>
    where
        Self: Sized,
    {
        for value in dst.iter_mut() {
            *value = Self::value_from_reader(reader)?;
        }
        Ok(())
    }
}

impl Numeric for i8 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_i8()
    }
}
impl Numeric for u8 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_u8()
    }

    fn values_from_reader<R: Read>(reader: &mut R, dst: &mut [Self]) -> Result<(), std::io::Error> {
        reader.read_exact(dst)?;
        Ok(())
    }
}
impl Numeric for i32 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_i32::<BigEndian>()
    }
}
impl Numeric for u32 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_u32::<BigEndian>()
    }
}
impl Numeric for f32 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_f32::<BigEndian>()
    }
}
impl Numeric for u64 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_u64::<BigEndian>()
    }
}
impl Numeric for i16 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_i16::<BigEndian>()
    }
}
impl Numeric for u16 {
    fn value_from_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_u16::<BigEndian>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    #[test]
    fn it_works() -> Result<(), KlvError> {
        //let bytes = include_bytes!("../test_files/gpmf.bin");
        let bytes = include_bytes!("../test_files/sample_60.bin");

        {
            let mut bytes = Cursor::new(bytes);

            let klvs = Klv::from_reader(&mut bytes)?;
            for klv in klvs {
                println!("{:?} {:?}", klv.header(), klv.value());
            }
        }

        Ok(())
    }
}
