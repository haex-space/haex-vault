//! Tools that extensions offer to the built-in agent (spec 018, ADR-0004 direction A): holzi is the
//! MCP client, the extension's frame the server, and the messages travel as JSON through the
//! frontend to the frame's port.

pub mod commands;
pub mod link;
pub mod slot;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "link_tests.rs"]
mod link_tests;

#[cfg(test)]
#[path = "contract_tests.rs"]
mod contract_tests;

#[cfg(test)]
#[path = "commands_tests.rs"]
mod commands_tests;
