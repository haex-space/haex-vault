use super::gguf_header::read_architecture;

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
