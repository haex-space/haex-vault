//! A place in memory for tests of the plan, the executor and runs (task T017): files with content
//! and time, folders, a switch that makes the place unreachable, failures per path, and counters
//! of the changing calls.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::place::{split, trash_path, unreachable, Facts, Listing, Place, Trashed};
use super::place_device::hex;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode};

#[derive(Default)]
struct Inner {
    files: BTreeMap<String, (Vec<u8>, i64)>,
    folders: BTreeSet<String>,
    unreachable: bool,
    failing: BTreeSet<String>,
    calls: BTreeMap<&'static str, usize>,
}

/// A place in memory.
#[derive(Default)]
pub struct MemPlace {
    inner: Mutex<Inner>,
}

impl MemPlace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts a file with `content` and the time `mtime_ms`, and the folders above it.
    pub fn put(&self, path: &str, content: &[u8], mtime_ms: i64) {
        let mut inner = self.inner.lock().unwrap();
        let mut folder = split(path).0;
        while !folder.is_empty() {
            inner.folders.insert(folder.to_owned());
            folder = split(folder).0;
        }
        inner
            .files
            .insert(path.to_owned(), (content.to_vec(), mtime_ms));
    }

    /// The content of a file.
    pub fn content(&self, path: &str) -> Option<Vec<u8>> {
        let inner = self.inner.lock().unwrap();
        inner.files.get(path).map(|(content, _)| content.clone())
    }

    /// Every file path.
    pub fn paths(&self) -> Vec<String> {
        self.inner.lock().unwrap().files.keys().cloned().collect()
    }

    /// Makes the whole place unreachable, or reachable again.
    pub fn set_unreachable(&self, unreachable: bool) {
        self.inner.lock().unwrap().unreachable = unreachable;
    }

    /// Makes every call on `path` fail.
    pub fn fail(&self, path: &str) {
        self.inner.lock().unwrap().failing.insert(path.to_owned());
    }

    /// How often the changing call `name` (`rename`, `delete`, `trash`, `create_folder`,
    /// `write_text`) ran.
    pub fn calls(&self, name: &str) -> usize {
        self.inner
            .lock()
            .unwrap()
            .calls
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    fn enter(
        &self,
        call: &'static str,
        path: &str,
    ) -> Result<std::sync::MutexGuard<'_, Inner>, FilesError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.unreachable {
            return Err(unreachable("the test place is unreachable"));
        }
        if inner.failing.contains(path) {
            return Err(FilesError::new(
                FilesErrorCode::NoAccess,
                "the test path fails",
            ));
        }
        *inner.calls.entry(call).or_default() += 1;
        Ok(inner)
    }

    fn move_inner(inner: &mut Inner, from: &str, to: &str) -> Result<(), FilesError> {
        let file = inner
            .files
            .remove(from)
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
        inner.files.insert(to.to_owned(), file);
        Ok(())
    }
}

fn facts(content: &[u8], mtime_ms: i64) -> Facts {
    Facts {
        size: content.len() as u64,
        mtime_ms,
        hash: None,
    }
}

#[async_trait]
impl Place for MemPlace {
    async fn list_tree(&self, _cancel: &CancellationToken) -> Result<Listing, FilesError> {
        let inner = self.enter("list_tree", "")?;
        Ok(Listing {
            files: inner
                .files
                .iter()
                .map(|(path, (content, mtime))| (path.clone(), facts(content, *mtime)))
                .collect(),
            folders: inner.folders.clone(),
            skipped: Vec::new(),
        })
    }

    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError> {
        let inner = self.enter("stat", path)?;
        Ok(inner
            .files
            .get(path)
            .map(|(content, mtime)| facts(content, *mtime)))
    }

    async fn hash(&self, path: &str, _cancel: &CancellationToken) -> Result<String, FilesError> {
        let inner = self.enter("hash", path)?;
        let (content, _) = inner
            .files
            .get(path)
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
        Ok(hex(&Sha256::digest(content)))
    }

    async fn create_folder(&self, path: &str) -> Result<(), FilesError> {
        let mut inner = self.enter("create_folder", path)?;
        inner.folders.insert(path.to_owned());
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError> {
        let mut inner = self.enter("rename", from)?;
        Self::move_inner(&mut inner, from, to)
    }

    async fn delete(&self, path: &str) -> Result<(), FilesError> {
        let mut inner = self.enter("delete", path)?;
        inner
            .files
            .remove(path)
            .map(drop)
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))
    }

    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError> {
        let mut inner = self.enter("trash", path)?;
        let rule = trash_path(run, path);
        Self::move_inner(&mut inner, path, &rule)?;
        Ok(Trashed::Rule(rule))
    }

    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError> {
        let inner = self.enter("read_text", path)?;
        match inner.files.get(path) {
            None => Ok(None),
            Some((content, _)) if content.len() > max => Err(FilesError::new(
                FilesErrorCode::TooLarge,
                "the file is too large",
            )),
            Some((content, _)) => Ok(Some(String::from_utf8_lossy(content).into_owned())),
        }
    }

    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError> {
        let mut inner = self.enter("write_text", path)?;
        let mtime = inner.files.get(path).map_or(1, |(_, mtime)| mtime + 1);
        inner
            .files
            .insert(path.to_owned(), (text.as_bytes().to_vec(), mtime));
        Ok(())
    }

    fn side(&self) -> Side {
        Side::Device
    }

    fn full_path(&self, path: &str) -> String {
        format!("mem:{path}")
    }
}

#[cfg(test)]
#[path = "test_support_tests.rs"]
mod tests;
