//! How many tool rounds one turn may use: the vault setting "Tool rounds per answer" (KI → Agent
//! profiles), so the cap can change without a new build. Without a stored value, or with one that is
//! not a whole number in range, the turn uses [`MAX_TOOL_ROUNDS`].

use super::tool_round::MAX_TOOL_ROUNDS;
use crate::storage::preferences::{self, PrefScope};
use crate::vault_gate::VaultDb;

/// The preference key.
pub const TOOL_ROUNDS_KEY: &str = "chat.maxToolRounds";
/// The fewest rounds the setting takes: fewer would end most answers before they start.
pub const MIN_TOOL_ROUNDS: usize = 1;
/// The most the setting takes: the cap stops a loop, and a loop of this size is long enough.
pub const MAX_TOOL_ROUNDS_SETTING: usize = 200;

/// The number `value` stands for, if it is an allowed stored value of [`TOOL_ROUNDS_KEY`]: plain
/// digits, from [`MIN_TOOL_ROUNDS`] to [`MAX_TOOL_ROUNDS_SETTING`].
pub fn parse_tool_rounds(value: &str) -> Option<usize> {
    let rounds: usize = value.parse().ok()?;
    ((MIN_TOOL_ROUNDS..=MAX_TOOL_ROUNDS_SETTING).contains(&rounds) && rounds.to_string() == value)
        .then_some(rounds)
}

/// Whether `value` is an allowed stored value of [`TOOL_ROUNDS_KEY`].
pub fn is_valid_tool_rounds(value: &str) -> bool {
    parse_tool_rounds(value).is_some()
}

/// The cap in effect for the next round, read from the open vault; [`MAX_TOOL_ROUNDS`] while none is
/// stored or it cannot be read. Read per round, so a change applies to a turn that is running.
pub async fn tool_rounds(db: &VaultDb) -> usize {
    let stored = db
        .read(|q| preferences::get(q, PrefScope::Vault, TOOL_ROUNDS_KEY))
        .await;
    match stored {
        Ok(Some(value)) => parse_tool_rounds(&value).unwrap_or(MAX_TOOL_ROUNDS),
        Ok(None) => MAX_TOOL_ROUNDS,
        Err(error) => {
            log::warn!(
                "chat: the tool round limit could not be read, using {MAX_TOOL_ROUNDS}: {error}"
            );
            MAX_TOOL_ROUNDS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_in_range_is_taken() {
        assert_eq!(parse_tool_rounds("1"), Some(1));
        assert_eq!(parse_tool_rounds("30"), Some(30));
        assert_eq!(parse_tool_rounds("200"), Some(200));
    }

    #[test]
    fn anything_else_is_refused() {
        for value in ["", "0", "201", "-5", "3.5", "030", " 30", "30 ", "many"] {
            assert_eq!(parse_tool_rounds(value), None, "{value:?}");
            assert!(!is_valid_tool_rounds(value), "{value:?}");
        }
    }
}
