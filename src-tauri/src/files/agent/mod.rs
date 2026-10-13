//! Files for agents (spec 044 US6, contracts/agent-actions.md): the native executor of the
//! `files.*` actions, what it needs from the app, and the question for a storage permission or an
//! encrypted folder (spec 048 US6).

pub mod commands;
pub mod env;
pub mod exec;
mod exec_write;
mod folders;
pub mod prompt;
mod reach;
mod read;

#[cfg(test)]
mod exec_tests;
#[cfg(test)]
mod exec_write_tests;
#[cfg(test)]
mod folders_device_tests;
#[cfg(test)]
mod folders_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod test_env;
