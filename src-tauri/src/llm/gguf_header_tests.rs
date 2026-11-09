use super::gguf_header::read_architecture;
#[cfg(feature = "llm-llamacpp")]
use super::gguf_header::{read_summary, GgufSummary};

fn string(value: &str) -> Vec<u8> {
    let mut out = (value.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(value.as_bytes());
    out
}

/// A GGUF header with an array and a number before the architecture, as real files have.
fn header(architecture: &str) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&3u64.to_le_bytes());
    out.extend(string("tokenizer.ggml.tokens"));
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&2u64.to_le_bytes());
    out.extend(string("a"));
    out.extend(string("bc"));
    out.extend(string("general.file_type"));
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&15u32.to_le_bytes());
    out.extend(string("general.architecture"));
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend(string(architecture));
    out
}

#[test]
fn reads_the_architecture_after_other_keys() {
    let bytes = header("qwen35");
    assert_eq!(read_architecture(&mut bytes.as_slice()).unwrap(), "qwen35");
}

#[test]
fn rejects_a_file_that_is_not_gguf() {
    assert!(read_architecture(&mut b"RIFF....".as_slice()).is_err());
}

#[test]
fn reports_a_truncated_header() {
    let bytes = header("qwen3");
    assert!(read_architecture(&mut &bytes[..bytes.len() - 10]).is_err());
}

#[cfg(feature = "llm-llamacpp")]
mod summary {
    use super::*;

    fn u32_entry(key: &str, value: u32) -> Vec<u8> {
        let mut out = string(key);
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
        out
    }

    /// A header with the shape keys of a small model, one per-layer array and a key of another architecture.
    fn shaped_header(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"GGUF".to_vec();
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&((entries.len() + 1) as u64).to_le_bytes());
        out.extend(string("general.architecture"));
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend(string("qwen3"));
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out
    }

    #[test]
    fn summary_keeps_the_shape_of_the_models_architecture() {
        let bytes = shaped_header(&[
            u32_entry("qwen3.block_count", 36),
            u32_entry("qwen3.embedding_length", 2560),
            u32_entry("qwen3.attention.head_count", 32),
            u32_entry("qwen3.attention.head_count_kv", 8),
            u32_entry("qwen3.attention.key_length", 128),
            u32_entry("qwen3.context_length", 40960),
            u32_entry("other.block_count", 99),
        ]);
        assert_eq!(
            read_summary(&mut bytes.as_slice()).unwrap(),
            GgufSummary {
                architecture: "qwen3".to_owned(),
                block_count: Some(36),
                embedding_length: Some(2560),
                head_count: Some(32),
                head_count_kv: Some(8),
                key_length: Some(128),
                context_length: Some(40960),
            }
        );
    }

    #[test]
    fn summary_leaves_a_per_layer_array_out() {
        let mut array = string("qwen3.attention.head_count_kv");
        array.extend_from_slice(&9u32.to_le_bytes());
        array.extend_from_slice(&4u32.to_le_bytes());
        array.extend_from_slice(&2u64.to_le_bytes());
        array.extend_from_slice(&8u32.to_le_bytes());
        array.extend_from_slice(&2u32.to_le_bytes());
        let bytes = shaped_header(&[array, u32_entry("qwen3.block_count", 4)]);
        let summary = read_summary(&mut bytes.as_slice()).unwrap();
        assert_eq!(summary.head_count_kv, None);
        assert_eq!(summary.block_count, Some(4));
    }

    #[test]
    fn summary_reports_a_truncated_header() {
        let bytes = shaped_header(&[u32_entry("qwen3.block_count", 36)]);
        assert!(read_summary(&mut &bytes[..bytes.len() - 2]).is_err());
    }
}
