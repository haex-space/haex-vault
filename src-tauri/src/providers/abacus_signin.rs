//! The client of the Abacus sign-in service: the device-code flow HaexVault runs on behalf of the
//! user, so no terminal and no variable is involved (spec 096 FR-005, research §3).
//!
//! Filled in by the sign-in client of spec 096 (T018).

#[cfg(test)]
#[path = "abacus_signin_tests.rs"]
mod tests;
