pub const MAX_VALUE_BYTES_LEN: usize = 10 * 1024 * 1024; // 10 MiB, matches ADR 0001 tx-cap

#[derive(Debug, thiserror::Error)]
#[error("value_bytes length {actual} exceeds max {max}")]
pub struct ValueBytesTooLarge {
    pub actual: usize,
    pub max: usize,
}

pub fn check_value_bytes_len(len: usize) -> Result<(), ValueBytesTooLarge> {
    if len > MAX_VALUE_BYTES_LEN {
        Err(ValueBytesTooLarge {
            actual: len,
            max: MAX_VALUE_BYTES_LEN,
        })
    } else {
        Ok(())
    }
}
