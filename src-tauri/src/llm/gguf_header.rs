//! Reads `general.architecture` (and the few sizes a memory estimate needs) from a GGUF header without
//! loading any tensors.
//!
//! The engine uses the architecture for diagnostics and reads the shape metadata for its memory estimate.

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
pub(crate) fn read_architecture(reader: &mut impl Read) -> io::Result<String> {
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

#[cfg(feature = "llm-llamacpp")]
mod summary {
    use std::collections::HashMap;
    use std::io::{self, Read};

    use super::{
        fixed_width, invalid, read_string, read_u32, read_u64, skip, skip_value, ARCHITECTURE_KEY,
        MAGIC, TYPE_ARRAY, TYPE_STRING,
    };

    /// What the header says about the model's shape, as far as the memory estimate of the llama.cpp engine
    /// (spec 070, plan D6) needs it. A value the file does not carry is `None`; a per-layer array is in the
    /// field of its own (`head_count_kv_layers`, `sliding_window_pattern`), and `head_count_kv` is then `None`.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct GgufSummary {
        pub architecture: String,
        pub block_count: Option<u64>,
        pub embedding_length: Option<u64>,
        pub head_count: Option<u64>,
        pub head_count_kv: Option<u64>,
        pub key_length: Option<u64>,
        pub context_length: Option<u64>,
        /// KV heads per layer, for models whose layers differ (Gemma 4: global layers have one).
        pub head_count_kv_layers: Option<Vec<u64>>,
        pub value_length: Option<u64>,
        /// Head sizes of the layers that look at a window only.
        pub key_length_swa: Option<u64>,
        pub value_length_swa: Option<u64>,
        /// How many tokens a windowed layer looks back.
        pub sliding_window: Option<u64>,
        /// Per layer: whether it looks at a window only (`true`) or at the whole context.
        pub sliding_window_pattern: Option<Vec<bool>>,
        /// Hybrid models (Qwen3.5): every n-th layer is an attention layer, the others keep a state of
        /// fixed size instead of a cache.
        pub full_attention_interval: Option<u64>,
        pub ssm_inner_size: Option<u64>,
        pub ssm_state_size: Option<u64>,
        /// The rope scaling the file declares for itself (`linear`, `yarn`, `none`), when it declares one.
        pub rope_scaling_type: Option<String>,
    }

    /// The `<architecture>.<suffix>` keys [`read_summary`] keeps as numbers.
    const SHAPE_SUFFIXES: [&str; 13] = [
        "block_count",
        "embedding_length",
        "attention.head_count",
        "attention.head_count_kv",
        "attention.key_length",
        "attention.value_length",
        "attention.key_length_swa",
        "attention.value_length_swa",
        "attention.sliding_window",
        "full_attention_interval",
        "ssm.inner_size",
        "ssm.state_size",
        "context_length",
    ];

    /// The keys that may be a per-layer array of integers.
    const ARRAY_SUFFIXES: [&str; 2] = [
        "attention.head_count_kv",
        "attention.sliding_window_pattern",
    ];

    /// The longest per-layer array that is read; models have well under a hundred layers.
    const MAX_LAYERS: u64 = 4096;

    /// Reads an integer value of any GGUF integer type; `None` for every other type (the value is consumed).
    fn read_uint(reader: &mut impl Read, value_type: u32) -> io::Result<Option<u64>> {
        let width = match value_type {
            // A boolean is one byte, which the window pattern of a model is made of.
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4 | 5 => 4,
            10 | 11 => 8,
            other => {
                skip_value(reader, other)?;
                return Ok(None);
            }
        };
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf[..width])?;
        Ok(Some(u64::from_le_bytes(buf)))
    }

    /// Reads an array of integers or booleans; `None` for any other array (its elements are consumed).
    fn read_uint_array(reader: &mut impl Read) -> io::Result<Option<Vec<u64>>> {
        let element_type = read_u32(reader)?;
        let count = read_u64(reader)?;
        let integer = matches!(element_type, 0..=5 | 7 | 10 | 11);
        match fixed_width(element_type)? {
            Some(_) if integer && count <= MAX_LAYERS => {
                let mut values = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    values.push(read_uint(reader, element_type)?.unwrap_or(0));
                }
                Ok(Some(values))
            }
            Some(width) => {
                skip(reader, width.saturating_mul(count))?;
                Ok(None)
            }
            None => {
                for _ in 0..count {
                    skip_value(reader, element_type)?;
                }
                Ok(None)
            }
        }
    }

    /// Walks all metadata key/value pairs of a GGUF stream and keeps the architecture and the shape numbers.
    pub fn read_summary(reader: &mut impl Read) -> io::Result<GgufSummary> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(invalid("not a GGUF file"));
        }
        let _version = read_u32(reader)?;
        let _tensor_count = read_u64(reader)?;
        let kv_count = read_u64(reader)?;
        let mut architecture = None;
        let mut numbers: HashMap<String, u64> = HashMap::new();
        let mut arrays: HashMap<String, Vec<u64>> = HashMap::new();
        let mut scaling_types: HashMap<String, String> = HashMap::new();
        for _ in 0..kv_count {
            let key = read_string(reader)?;
            let value_type = read_u32(reader)?;
            if key == ARCHITECTURE_KEY {
                if value_type != TYPE_STRING {
                    return Err(invalid("general.architecture is not a string"));
                }
                architecture = Some(read_string(reader)?);
            } else if value_type == TYPE_STRING && key.ends_with(".rope.scaling.type") {
                scaling_types.insert(key, read_string(reader)?);
            } else if value_type == TYPE_ARRAY
                && ARRAY_SUFFIXES
                    .iter()
                    .any(|suffix| key.ends_with(&format!(".{suffix}")))
            {
                if let Some(values) = read_uint_array(reader)? {
                    arrays.insert(key, values);
                }
            } else if SHAPE_SUFFIXES
                .iter()
                .any(|suffix| key.ends_with(&format!(".{suffix}")))
            {
                if let Some(value) = read_uint(reader, value_type)? {
                    numbers.insert(key, value);
                }
            } else {
                skip_value(reader, value_type)?;
            }
        }
        let architecture =
            architecture.ok_or_else(|| invalid("GGUF has no general.architecture"))?;
        let number = |suffix: &str| numbers.get(&format!("{architecture}.{suffix}")).copied();
        let array = |suffix: &str| arrays.get(&format!("{architecture}.{suffix}")).cloned();
        Ok(GgufSummary {
            rope_scaling_type: scaling_types
                .get(&format!("{architecture}.rope.scaling.type"))
                .cloned(),
            head_count_kv_layers: array("attention.head_count_kv"),
            value_length: number("attention.value_length"),
            key_length_swa: number("attention.key_length_swa"),
            value_length_swa: number("attention.value_length_swa"),
            sliding_window: number("attention.sliding_window"),
            full_attention_interval: number("full_attention_interval"),
            ssm_inner_size: number("ssm.inner_size"),
            ssm_state_size: number("ssm.state_size"),
            sliding_window_pattern: array("attention.sliding_window_pattern")
                .map(|flags| flags.into_iter().map(|flag| flag != 0).collect()),
            block_count: number("block_count"),
            embedding_length: number("embedding_length"),
            head_count: number("attention.head_count"),
            head_count_kv: number("attention.head_count_kv"),
            key_length: number("attention.key_length"),
            context_length: number("context_length"),
            architecture,
        })
    }
}

#[cfg(feature = "llm-llamacpp")]
pub use summary::{read_summary, GgufSummary};
