//! Reads `general.architecture` from a GGUF header without loading any tensors.
//!
//! mistralrs 0.8.1 `unwrap()`s the architecture lookup while building a GGUF model, so a file of
//! an architecture it does not know (Qwen3.5 is `qwen35`) panics inside the build and the load
//! never returns. The loader asks here first and refuses with a message instead.

use std::io::{self, Read};

const MAGIC: &[u8; 4] = b"GGUF";
const ARCHITECTURE_KEY: &str = "general.architecture";
/// GGUF value type ids (`gguf_metadata_value_type`).
const TYPE_STRING: u32 = 8;
const TYPE_ARRAY: u32 = 9;
/// Upper bound for one string, so a corrupt length cannot allocate gigabytes.
const MAX_STRING_BYTES: u64 = 1 << 20;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_string(reader: &mut impl Read) -> io::Result<String> {
    let len = read_u64(reader)?;
    if len > MAX_STRING_BYTES {
        return Err(invalid("GGUF string too long"));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| invalid("GGUF string is not utf-8"))
}

/// Byte width of a fixed-size value type, `None` for strings and arrays.
fn fixed_width(value_type: u32) -> io::Result<Option<u64>> {
    match value_type {
        0 | 1 | 7 => Ok(Some(1)),
        2 | 3 => Ok(Some(2)),
        4..=6 => Ok(Some(4)),
        10..=12 => Ok(Some(8)),
        TYPE_STRING | TYPE_ARRAY => Ok(None),
        _ => Err(invalid("unknown GGUF value type")),
    }
}

fn skip(reader: &mut impl Read, bytes: u64) -> io::Result<()> {
    let copied = io::copy(&mut reader.take(bytes), &mut io::sink())?;
    if copied == bytes {
        Ok(())
    } else {
        Err(io::ErrorKind::UnexpectedEof.into())
    }
}

fn skip_value(reader: &mut impl Read, value_type: u32) -> io::Result<()> {
    if let Some(width) = fixed_width(value_type)? {
        return skip(reader, width);
    }
    if value_type == TYPE_STRING {
        let len = read_u64(reader)?;
        return skip(reader, len);
    }
    let element_type = read_u32(reader)?;
    let count = read_u64(reader)?;
    match fixed_width(element_type)? {
        Some(width) => skip(reader, width.saturating_mul(count)),
        None => {
            for _ in 0..count {
                skip_value(reader, element_type)?;
            }
            Ok(())
        }
    }
}

/// Walks the metadata key/value pairs of a GGUF stream up to `general.architecture`.
pub(super) fn read_architecture(reader: &mut impl Read) -> io::Result<String> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("not a GGUF file"));
    }
    let _version = read_u32(reader)?;
    let _tensor_count = read_u64(reader)?;
    let kv_count = read_u64(reader)?;
    for _ in 0..kv_count {
        let key = read_string(reader)?;
        let value_type = read_u32(reader)?;
        if key == ARCHITECTURE_KEY {
            if value_type != TYPE_STRING {
                return Err(invalid("general.architecture is not a string"));
            }
            return read_string(reader);
        }
        skip_value(reader, value_type)?;
    }
    Err(invalid("GGUF has no general.architecture"))
}
