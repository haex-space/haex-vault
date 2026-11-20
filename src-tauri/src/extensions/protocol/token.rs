//! Start tokens of frames (research R13): random, bound to one frame session, compared in
//! constant time.

/// A new token: 32 random bytes as lower-case hex.
pub fn mint() -> String {
    let mut bytes = [0u8; 32];
    // As in `sync::keys`: haex-vault cannot run without the operating system's random source.
    getrandom::fill(&mut bytes).expect("the operating system's random source is available");
    crate::sync::keys::hex(&bytes)
}

/// Whether `given` equals `expected`, in time that does not depend on where they differ.
pub fn matches(expected: &str, given: &str) -> bool {
    expected.len() == given.len()
        && expected
            .bytes()
            .zip(given.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
