//! An environment for the executor tests: a device folder with one of holzi's own places in it,
//! grants in memory, storages on a [`FakeStore`] (one bucket per storage id) and a window that
//! answers the question for a storage as the test says.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::FilesAgent;
use super::prompt::{FilesAgentChoice, FilesAgentPermissionRequest, PermissionPrompt};
use crate::chat::tools::native_action::{NativeError, NativeExecutor, NativeReply};
use crate::files::access::AgentGrants;
use crate::files::browser_commands::{KnownPlace, StorageSource};
use crate::files::local::OwnPlaces;
use crate::files::permissions::{AgentFileStatus, BUILTIN_AGENT};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Removal;
use crate::files::FilesError;
use crate::passwords::access::Caller;
use crate::remote_storage::test_support::{access, FakeStore};

pub struct FakeEnv {
    pub own: OwnPlaces,
    pub known: Vec<KnownPlace>,
    pub grants: Mutex<AgentGrants>,
    pub stored: Mutex<Vec<(String, AgentFileStatus)>>,
    pub storages: Vec<StorageSource>,
    pub store: Arc<FakeStore>,
    pub prompt: PermissionPrompt,
    pub asked: Arc<Mutex<Vec<FilesAgentPermissionRequest>>>,
}

#[async_trait]
impl AgentEnv for FakeEnv {
    fn own(&self) -> OwnPlaces {
        self.own.clone()
    }

    fn known(&self) -> Vec<KnownPlace> {
        self.known.clone()
    }

    async fn grants(&self, _agent_id: &str) -> Result<AgentGrants, FilesError> {
        Ok(self.grants.lock().expect("grants").clone())
    }

    async fn store_grant(
        &self,
        _agent_id: &str,
        storage_id: &str,
        status: AgentFileStatus,
    ) -> Result<(), FilesError> {
        let grant = match status {
            AgentFileStatus::Read => crate::files::access::StorageGrant::Read,
            AgentFileStatus::ReadWrite => crate::files::access::StorageGrant::ReadWrite,
            _ => crate::files::access::StorageGrant::Denied,
        };
        self.grants
            .lock()
            .expect("grants")
            .storages
            .insert(storage_id.to_owned(), grant);
        self.stored
            .lock()
            .expect("stored")
            .push((storage_id.to_owned(), status));
        Ok(())
    }

    async fn storages(&self) -> Result<Vec<StorageSource>, FilesError> {
        Ok(self.storages.clone())
    }

    async fn open_storage(&self, storage_id: &str) -> Result<StorageFiles, FilesError> {
        Ok(StorageFiles::new(self.store.clone(), access(storage_id)))
    }

    fn prompt(&self) -> &PermissionPrompt {
        &self.prompt
    }
}

/// The device folder of a test and the agent over it.
pub struct Scene {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub agent: FilesAgent<FakeEnv>,
}

impl Scene {
    /// `root` with `notiz.txt`, `steuer/brief-2025.txt`, `foto.jpg`, `bin.dat`, an empty `b/` and
    /// holzi's own place `holzi/` holding `vault.db` and `brief-vault.txt`; the storage `s1`
    /// ("Fotos") holds `a.txt`. A question for a storage gets `answer`; `None` is a missing window.
    pub fn new(answer: Option<FilesAgentChoice>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = fs::canonicalize(dir.path()).expect("canonical");
        fs::write(root.join("notiz.txt"), "Hallo Welt").expect("write");
        fs::create_dir_all(root.join("steuer")).expect("mkdir");
        fs::write(root.join("steuer/brief-2025.txt"), "Finanzamt").expect("write");
        fs::write(root.join("foto.jpg"), [0xff, 0xd8, 0xff]).expect("write");
        fs::write(root.join("bin.dat"), [b'a', 0, b'b']).expect("write");
        fs::create_dir_all(root.join("b")).expect("mkdir");
        fs::create_dir_all(root.join("holzi")).expect("mkdir");
        fs::write(root.join("holzi/vault.db"), "secret vault").expect("write");
        fs::write(root.join("holzi/brief-vault.txt"), "secret").expect("write");

        let store = Arc::new(FakeStore::new());
        store.insert("s1", "a.txt", b"im Speicher");
        let prompt = PermissionPrompt::with_timeout(Duration::from_secs(5));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (seen, answering) = (asked.clone(), prompt.clone());
        prompt.set_emitter(Arc::new(move |request: &FilesAgentPermissionRequest| {
            seen.lock().expect("asked").push(request.clone());
            let Some(choice) = answer else {
                return false;
            };
            let (answering, id) = (answering.clone(), request.request_id.clone());
            tokio::spawn(async move {
                answering.answer(&id, choice);
            });
            true
        }));
        let env = FakeEnv {
            own: OwnPlaces::new(vec![root.join("holzi")]),
            known: vec![KnownPlace {
                name: "Steuer".to_owned(),
                path: root.join("steuer").to_string_lossy().into_owned(),
            }],
            grants: Mutex::new(AgentGrants::default()),
            stored: Mutex::new(Vec::new()),
            storages: vec![StorageSource {
                id: "s1".to_owned(),
                name: "Fotos".to_owned(),
            }],
            store,
            prompt,
            asked,
        };
        let agent = FilesAgent {
            env,
            agent_id: BUILTIN_AGENT.to_owned(),
            caller: Caller::BuiltinAgent,
            removal: Removal::Permanent,
        };
        Self {
            _dir: dir,
            root,
            agent,
        }
    }

    /// `name` below the device folder, as an agent spells it.
    pub fn at(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub async fn run(&self, action: &str, input: Value) -> Result<Value, NativeError> {
        Ok(self.reply(action, input).await?.result)
    }

    /// The whole answer, with the images it shows the model.
    pub async fn reply(&self, action: &str, input: Value) -> Result<NativeReply, NativeError> {
        self.agent
            .run(action, input, &CancellationToken::new())
            .await
    }

    pub fn asked(&self) -> usize {
        self.agent.env.asked.lock().expect("asked").len()
    }
}

/// The names of the entries of a `files.list` or the hits of a `files.search`.
pub fn names(result: &Value, key: &str) -> Vec<String> {
    let mut names: Vec<String> = result[key]
        .as_array()
        .expect("a list")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name").to_owned())
        .collect();
    names.sort();
    names
}
