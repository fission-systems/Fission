use super::parser::FidbfParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawFieldType {
    Byte,
    Short,
    Int,
    Long,
    String,
    Binary,
    Boolean,
    Fixed(usize),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum RawValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    String(Option<String>),
    Binary(Option<Vec<u8>>),
    Boolean(bool),
    Fixed(Vec<u8>),
}

impl RawFieldType {
    pub(crate) fn from_code(code: u8) -> Result<Self, FidbfParseError> {
        // Ghidra uses the low nibble for the concrete Field type.  Some legacy
        // schemas set the high nibble to mark index fields; the stored value
        // encoding is still the low-nibble field type.
        let ty = code & 0x0f;
        match ty {
            0 => Ok(Self::Byte),
            1 => Ok(Self::Short),
            2 => Ok(Self::Int),
            3 => Ok(Self::Long),
            4 => Ok(Self::String),
            5 => Ok(Self::Binary),
            6 => Ok(Self::Boolean),
            7 => Ok(Self::Fixed(10)),
            other => Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
                "unsupported DB field type code {other}"
            ))),
        }
    }

    pub(crate) fn fixed_size(self) -> Option<usize> {
        match self {
            Self::Byte | Self::Boolean => Some(1),
            Self::Short => Some(2),
            Self::Int => Some(4),
            Self::Long => Some(8),
            Self::Fixed(size) => Some(size),
            Self::String | Self::Binary => None,
        }
    }
}

pub(crate) fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, FidbfParseError> {
    let slice = checked_slice(bytes, offset, 4)?;
    Ok(i32::from_be_bytes(slice.try_into().expect("slice length")))
}

pub(crate) fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, FidbfParseError> {
    let slice = checked_slice(bytes, offset, 8)?;
    Ok(i64::from_be_bytes(slice.try_into().expect("slice length")))
}

pub(crate) fn checked_slice(
    bytes: &[u8],
    offset: usize,
    len: usize,
) -> Result<&[u8], FidbfParseError> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| FidbfParseError::MalformedRawFidDatabase("offset overflow".to_string()))?;
    bytes.get(offset..end).ok_or_else(|| {
        FidbfParseError::MalformedRawFidDatabase(format!(
            "raw DB read out of bounds at offset {offset} length {len}"
        ))
    })
}

pub(crate) fn read_value(
    ty: RawFieldType,
    bytes: &[u8],
    offset: usize,
) -> Result<(RawValue, usize), FidbfParseError> {
    match ty {
        RawFieldType::Byte => {
            let value = *checked_slice(bytes, offset, 1)?
                .first()
                .expect("slice length") as i8;
            Ok((RawValue::Byte(value), offset + 1))
        }
        RawFieldType::Boolean => {
            let value = *checked_slice(bytes, offset, 1)?
                .first()
                .expect("slice length")
                != 0;
            Ok((RawValue::Boolean(value), offset + 1))
        }
        RawFieldType::Short => {
            let slice = checked_slice(bytes, offset, 2)?;
            Ok((
                RawValue::Short(i16::from_be_bytes(slice.try_into().expect("slice length"))),
                offset + 2,
            ))
        }
        RawFieldType::Int => {
            let value = read_i32(bytes, offset)?;
            Ok((RawValue::Int(value), offset + 4))
        }
        RawFieldType::Long => {
            let value = read_i64(bytes, offset)?;
            Ok((RawValue::Long(value), offset + 8))
        }
        RawFieldType::String => {
            let len = read_i32(bytes, offset)?;
            let mut next = offset + 4;
            if len < 0 {
                return Ok((RawValue::String(None), next));
            }
            let len = usize::try_from(len).map_err(|_| {
                FidbfParseError::MalformedRawFidDatabase("negative string length".to_string())
            })?;
            let raw = checked_slice(bytes, next, len)?;
            let value = String::from_utf8(raw.to_vec()).map_err(|err| {
                FidbfParseError::MalformedRawFidDatabase(format!(
                    "invalid UTF-8 string field: {err}"
                ))
            })?;
            next += len;
            Ok((RawValue::String(Some(value)), next))
        }
        RawFieldType::Binary => {
            let len = read_i32(bytes, offset)?;
            let mut next = offset + 4;
            if len < 0 {
                return Ok((RawValue::Binary(None), next));
            }
            let len = usize::try_from(len).map_err(|_| {
                FidbfParseError::MalformedRawFidDatabase("negative binary length".to_string())
            })?;
            let raw = checked_slice(bytes, next, len)?.to_vec();
            next += len;
            Ok((RawValue::Binary(Some(raw)), next))
        }
        RawFieldType::Fixed(size) => {
            let raw = checked_slice(bytes, offset, size)?.to_vec();
            Ok((RawValue::Fixed(raw), offset + size))
        }
    }
}
