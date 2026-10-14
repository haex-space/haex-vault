//! Tools that extensions offer to the built-in agent (spec 018, ADR-0004 direction A): holzi is the
//! MCP client, the extension's frame the server, and the messages travel as JSON through the
//! frontend to the frame's port.

pub mod commands;
pub mod defs;
pub mod frame_request;
pub mod link;
pub mod registration;
pub mod slot;
pub mod tool;

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

#[cfg(test)]
#[path = "frame_request_tests.rs"]
mod frame_request_tests;

#[cfg(test)]
#[path = "defs_tests.rs"]
mod defs_tests;

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tool_tests;

#[cfg(test)]
#[path = "registration_tests.rs"]
mod registration_tests;
