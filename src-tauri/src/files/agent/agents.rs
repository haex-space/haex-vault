//! The file actions of every agent (spec 021 US5): the built-in agent's executor and one per
//! external agent, made on its first call. Each runs as its own agent with its own memory of the
//! answers and opaque names it was given; a caller that is neither gets nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::{FilesAgent, FILE_ACTION_IDS};
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};

/// Makes the executor of one external agent from its id.
type MakeExternal<E> = Box<dyn Fn(&str) -> FilesAgent<E> + Send + Sync>;

pub struct FilesAgents<E> {
    builtin: Arc<FilesAgent<E>>,
    make: MakeExternal<E>,
    external: Mutex<HashMap<String, Arc<FilesAgent<E>>>>,
}

impl<E: AgentEnv> FilesAgents<E> {
    /// `builtin` is the built-in agent's executor, which the app also keeps for its own commands;
    /// `make` builds an external agent's ([`FilesAgent::external`]).
    pub fn new(
        builtin: Arc<FilesAgent<E>>,
        make: impl Fn(&str) -> FilesAgent<E> + Send + Sync + 'static,
    ) -> Self {
        Self {
            builtin,
            make: Box::new(make),
            external: Mutex::default(),
        }
    }

    fn agent_for(&self, caller: &Caller) -> Option<Arc<FilesAgent<E>>> {
        match caller {
            Caller::BuiltinAgent => Some(Arc::clone(&self.builtin)),
            // ponytail: one executor per external agent for the life of the vault session; a
            // revoked agent's stays until the vault closes (it holds no grant, only its memory).
            Caller::ExternalAgent { id } => Some(Arc::clone(
                self.external
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(id.clone())
                    .or_insert_with(|| Arc::new((self.make)(id))),
            )),
            _ => None,
        }
    }
}

#[async_trait]
impl<E: AgentEnv + 'static> NativeExecutor for FilesAgents<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        FILE_ACTION_IDS
    }

    fn serves_external_agents(&self) -> bool {
        true
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        let Some(agent) = self.agent_for(&call.caller) else {
            return Err(NativeError::new(
                "forbidden_for_agents",
                "This action is not available to this agent.",
            ));
        };
        agent.run(action_id, input, call, cancel).await
    }
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
