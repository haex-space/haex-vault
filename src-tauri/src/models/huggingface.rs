//! Free Hugging Face model discovery and installation boundary (spec 005).
//!
//! Owns every anonymous HTTP call to the public Hugging Face Hub API,
//! repository/filename/revision validation, GGUF filtering, and the pure
//! normalization (quantization, tokenizer, provenance) that turns a raw
//! Hub response into the contract types in
//! `specs/005-huggingface-model-discovery/contracts/tauri-commands.md`.
//! `commands.rs` calls into this module; it never talks to `reqwest`
//! directly for Hugging Face traffic.
//!
//! [`HfClient::base_url`] is overridable so tests point it at a local
//! `wiremock` server instead of the real Hub — search, details, revision
//! resolution and GGUF-header parsing are all exercised without a real
//! network call.
//!
//! Maintainability exception (spaex 500-LoC rule): this is one cohesive
//! Hugging Face boundary containing validation, transport DTOs, GGUF header
//! parsing, normalization and preview orchestration. The concrete follow-up
//! split is to move the GGUF parser and raw transport DTOs into dedicated
//! modules after the feature contract is stable; keeping them together for
//! this implementation avoids duplicating private parsing/normalization
//! types across two network boundaries.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::StreamExt;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{HolziError, Result};
use crate::hardware::{classify, Fit, HardwareInfo, ModelFitInputs};

use super::hash::sha256_bytes_hex;
use super::paths;

/// Production Hugging Face Hub host. The only host [`resolve_download_url`]
/// accepts (Entscheidung: "HTTPS-Host ... Validierung").
pub const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// Research.md Entscheidung 10: "höchstens 20 normalisierte Repository-
/// Treffer pro Seite". A client-requested limit is clamped to this value.
pub const DEFAULT_LIMIT: usize = 20;

/// The branch Holzi tracks when the caller does not pin an explicit
/// revision. Recorded as `revisionRef` so `check_huggingface_model_updates`
/// has something to compare against later.
const DEFAULT_REVISION_REF: &str = "main";

/// Wall-clock budget for a single Hub metadata request. Generous relative
/// to `download::download_to_file`'s streaming transfer, which has no
/// fixed timeout — this only bounds the small JSON/range calls here.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Only host allowed as the target of an actual file download
/// ([`resolve_download_url`]). Metadata calls go through the
/// caller-supplied, test-overridable `base_url` instead — see
/// [`HfClient::new`].
const ALLOWED_DOWNLOAD_HOST: &str = "huggingface.co";

// ---------------------------------------------------------------------
// Contract types (specs/005.../data-model.md, contracts/tauri-commands.md)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataProvenanceSource {
    HubMetadata,
    FilenameHeuristic,
    GgufHeader,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProvenance {
    pub quantization: MetadataProvenanceSource,
    pub context_window: MetadataProvenanceSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuggingFaceFileCandidate {
    pub repo_id: String,
    pub filename: String,
    pub revision: String,
    pub revision_ref: Option<String>,
    pub size_bytes: Option<u64>,
    pub quantization: Option<String>,
    pub context_window: Option<u64>,
    pub tokenizer_repo: Option<String>,
    pub tokenizer_required: bool,
    pub fit: Fit,
    pub catalog_match: bool,
    pub catalog_entry_id: Option<String>,
    pub metadata_provenance: MetadataProvenance,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuggingFaceModelResult {
    pub repo_id: String,
    pub display_name: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub downloads: Option<u64>,
    pub files: Vec<HuggingFaceFileCandidate>,
    pub source_revision: Option<String>,
    pub revision_ref: Option<String>,
}

/// Return shape of `preview_huggingface_install` (contracts/tauri-commands.md).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub model_id: String,
    pub name: String,
    pub repo_id: String,
    pub filename: String,
    pub revision: String,
    pub revision_ref: Option<String>,
    pub size_bytes: Option<u64>,
    pub quantization: Option<String>,
    pub context_window: Option<u64>,
    pub tokenizer_repo: Option<String>,
    pub tokenizer_required: bool,
    pub catalog_match: bool,
    pub catalog_entry_id: Option<String>,
    pub metadata_provenance: MetadataProvenance,
    pub fit: Fit,
    pub requires_explicit_too_big_confirmation: bool,
}

/// A resolved mutable-ref-to-commit-SHA outcome (research.md Entscheidung 7).
#[derive(Debug, Clone)]
pub struct RevisionResolution {
    pub sha: String,
    /// `None` for a direct SHA pin — there is nothing to track for updates.
    pub revision_ref: Option<String>,
}

// ---------------------------------------------------------------------
// Validation (spec 005 Phase 2 — path/host/input trust boundary)
// ---------------------------------------------------------------------

const MAX_REPO_SEGMENT_LEN: usize = 96;

/// Validates a public Hugging Face model repository id: exactly one
/// `owner/name` separator, each segment a bounded, printable,
/// non-traversal token.
pub fn validate_repo_id(repo_id: &str) -> Result<()> {
    let bad = || HolziError::InvalidInput {
        reason: format!("invalid Hugging Face repository id: {repo_id:?}"),
    };
    if repo_id.is_empty() || repo_id.len() > 2 * MAX_REPO_SEGMENT_LEN + 1 {
        return Err(bad());
    }
    if repo_id.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(bad());
    }
    let mut parts = repo_id.split('/');
    let owner = parts.next().ok_or_else(bad)?;
    let name = parts.next().ok_or_else(bad)?;
    if parts.next().is_some() {
        return Err(bad());
    }
    for segment in [owner, name] {
        if segment.is_empty() || segment.len() > MAX_REPO_SEGMENT_LEN {
            return Err(bad());
        }
        if segment == "." || segment == ".." {
            return Err(bad());
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(bad());
        }
    }
    Ok(())
}

/// Validates a user-supplied revision pointer (branch, tag, or full commit
/// SHA) before it is used in any HTTP path segment. Deliberately stricter
/// than a real branch name may allow (no `/`) — Hugging Face model
/// revisions in practice do not need one, and rejecting it keeps every
/// revision a single, unambiguous URL path segment.
pub fn validate_revision_ref(revision: &str) -> Result<()> {
    let bad = || HolziError::InvalidInput {
        reason: format!("invalid revision: {revision:?}"),
    };
    if revision.is_empty() || revision.len() > 200 {
        return Err(bad());
    }
    if revision
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(bad());
    }
    if revision.contains("..") || revision.contains('/') || revision.contains('\\') {
        return Err(bad());
    }
    Ok(())
}

/// `true` when `s` is a fully resolved git commit SHA: 40 lowercase hex
/// characters.
pub fn is_commit_sha(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// `true` when `filename` names an installable GGUF file.
pub fn is_gguf_filename(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with(".gguf")
}

/// Validates a GGUF filename against the same charset/traversal rules as
/// every other locally-stored model file (`models::paths`) plus the
/// `.gguf` extension requirement. Reuses `paths`'s validator directly —
/// spec 005 plan §"Graphify-Konsultation" is explicit that there must be
/// no second path validator.
pub fn validate_gguf_filename(filename: &str) -> Result<()> {
    paths::validate_filename(filename)?;
    if !is_gguf_filename(filename) {
        return Err(HolziError::UnsupportedFormat {
            filename: filename.to_string(),
        });
    }
    Ok(())
}

/// Validates that `url_str` is an `https` URL targeting the real
/// Hugging Face host. Applied only to the final constructed download URL
/// ([`resolve_download_url`]) — the configurable `HfClient::base_url` used
/// for search/details/tree/revision calls is a deliberate test seam and is
/// not subject to this allowlist.
fn validate_public_https_url(url_str: &str) -> Result<()> {
    let bad = |reason: String| HolziError::InvalidInput { reason };
    let parsed = Url::parse(url_str).map_err(|e| bad(format!("invalid URL: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(bad(format!("URL must use https: {url_str}")));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| bad(format!("URL has no host: {url_str}")))?;
    if host != ALLOWED_DOWNLOAD_HOST {
        return Err(bad(format!("URL host not allowed: {host}")));
    }
    Ok(())
}

/// Builds and validates the final `resolve/<sha>/<filename>` download URL.
/// Requires an already-resolved commit SHA — a mutable ref must go through
/// [`resolve_revision`] first.
pub fn resolve_download_url(
    base_url: &str,
    repo_id: &str,
    revision: &str,
    filename: &str,
) -> Result<String> {
    validate_repo_id(repo_id)?;
    if !is_commit_sha(revision) {
        return Err(HolziError::InvalidInput {
            reason: "download requires an already-resolved commit SHA".into(),
        });
    }
    validate_gguf_filename(filename)?;
    let url = build_path_url(
        base_url,
        &["".to_string()],
        repo_id,
        &["resolve", revision, filename],
    )?;
    validate_public_https_url(url.as_str())?;
    Ok(url.to_string())
}

// ---------------------------------------------------------------------
// Deterministic model id (data-model.md §"Persistenzänderung: models")
// ---------------------------------------------------------------------

const MODEL_ID_ENCODING_PREFIX: &str = "holzi-hf-model-id-v1";

/// Derives the stable local model id for a `(repoId, filename)` pair:
/// `hf-` + lowercase-hex SHA-256 of a versioned, length-prefixed encoding.
/// Two different `(repoId, filename)` pairs cannot collide because each
/// component is length-prefixed before concatenation.
pub fn derive_model_id(repo_id: &str, filename: &str) -> String {
    let mut buf =
        Vec::with_capacity(MODEL_ID_ENCODING_PREFIX.len() + 8 + repo_id.len() + filename.len());
    buf.extend_from_slice(MODEL_ID_ENCODING_PREFIX.as_bytes());
    for part in [repo_id, filename] {
        let bytes = part.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(bytes);
    }
    format!("hf-{}", sha256_bytes_hex(&buf))
}

// ---------------------------------------------------------------------
// Pure normalization: quantization, tokenizer, dedup/sort
// ---------------------------------------------------------------------

/// Known GGUF quantization tokens, used to recover a quantization label
/// from a filename when the Hub does not expose one structurally. Order
/// does not matter for correctness — [`normalize_quantization_from_filename`]
/// always keeps the longest boundary-matched token.
const KNOWN_QUANTIZATIONS: &[&str] = &[
    "IQ1_S", "IQ1_M", "IQ2_XXS", "IQ2_XS", "IQ2_S", "IQ2_M", "IQ3_XXS", "IQ3_XS", "IQ3_S", "IQ3_M",
    "IQ4_NL", "IQ4_XS", "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q3_K", "Q4_0", "Q4_1", "Q4_K_S",
    "Q4_K_M", "Q4_K", "Q5_0", "Q5_1", "Q5_K_S", "Q5_K_M", "Q5_K", "Q6_K", "Q8_0", "BF16", "F16",
    "F32",
];

/// Extracts a normalized quantization label from a GGUF filename by
/// matching known tokens on a word boundary (surrounded by start/end of
/// string or a non-alphanumeric character), preferring the longest match
/// so e.g. `Q3_K_M` wins over the `Q3_K` it also contains.
pub fn normalize_quantization_from_filename(filename: &str) -> Option<String> {
    let upper = filename.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut best: Option<&str> = None;
    for token in KNOWN_QUANTIZATIONS {
        if best.is_some_and(|b| token.len() <= b.len()) {
            continue;
        }
        // Every occurrence is checked, not just the first: a leading
        // non-boundary hit (`XQ4_0Y-Q4_0.gguf`) must not hide the real one.
        let matched = upper.match_indices(token).any(|(idx, _)| {
            let before_ok = idx == 0 || !bytes[idx - 1].is_ascii_alphanumeric();
            let after_idx = idx + token.len();
            let after_ok = after_idx >= bytes.len() || !bytes[after_idx].is_ascii_alphanumeric();
            before_ok && after_ok
        });
        if matched {
            best = Some(token);
        }
    }
    best.map(|s| s.to_string())
}

/// Resolves the tokenizer for a repository from its own sibling file list
/// (research.md Entscheidung 4: no guessed default). Only a `tokenizer.json`
/// inside the *same* repository counts as "sicher ermittelt" — a
/// `base_model` card hint is not proof the quantized repo re-published a
/// matching tokenizer, so it is intentionally not used as a source here.
pub fn resolve_tokenizer_repo(
    repo_id: &str,
    sibling_filenames: &[String],
) -> (Option<String>, bool) {
    if sibling_filenames.iter().any(|f| f == "tokenizer.json") {
        (Some(repo_id.to_string()), false)
    } else {
        (None, true)
    }
}

/// Deduplicates by `repoId` (first occurrence wins) and sorts
/// deterministically: descending downloads, then ascending `repoId` as a
/// stable tie-breaker independent of Hub response ordering.
pub fn dedupe_and_sort(results: &mut Vec<HuggingFaceModelResult>) {
    let mut seen: HashSet<String> = HashSet::new();
    results.retain(|r| seen.insert(r.repo_id.clone()));
    results.sort_by(|a, b| {
        b.downloads
            .unwrap_or(0)
            .cmp(&a.downloads.unwrap_or(0))
            .then_with(|| a.repo_id.cmp(&b.repo_id))
    });
}

fn catalog_match_for(repo_id: &str, filename: &str) -> (bool, Option<String>) {
    for entry in crate::catalog::entries() {
        if entry.hf_repo == repo_id && entry.hf_filename == filename {
            return (true, Some(entry.id.clone()));
        }
    }
    (false, None)
}

// ---------------------------------------------------------------------
// GGUF header parsing (research.md Entscheidung 9 — Range-Request fallback)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum GgufValueType {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
    Bool,
    String,
    Array,
    U64,
    I64,
    F64,
}

fn value_type_from_u32(v: u32) -> Option<GgufValueType> {
    Some(match v {
        0 => GgufValueType::U8,
        1 => GgufValueType::I8,
        2 => GgufValueType::U16,
        3 => GgufValueType::I16,
        4 => GgufValueType::U32,
        5 => GgufValueType::I32,
        6 => GgufValueType::F32,
        7 => GgufValueType::Bool,
        8 => GgufValueType::String,
        9 => GgufValueType::Array,
        10 => GgufValueType::U64,
        11 => GgufValueType::I64,
        12 => GgufValueType::F64,
        _ => return None,
    })
}

fn read_u32(buf: &[u8], pos: usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    if end > buf.len() {
        return None;
    }
    Some(u32::from_le_bytes(buf[pos..end].try_into().ok()?))
}

fn read_u64(buf: &[u8], pos: usize) -> Option<u64> {
    let end = pos.checked_add(8)?;
    if end > buf.len() {
        return None;
    }
    Some(u64::from_le_bytes(buf[pos..end].try_into().ok()?))
}

/// Reads a GGUF string (`u64` length prefix + UTF-8 bytes) at `pos`.
fn read_gguf_string(buf: &[u8], pos: usize) -> Option<(String, usize)> {
    let len = read_u64(buf, pos)? as usize;
    let start = pos.checked_add(8)?;
    let end = start.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    Some((String::from_utf8_lossy(&buf[start..end]).into_owned(), end))
}

/// Reads one typed KV value at `pos`. Returns its integer interpretation
/// when the type is an integer scalar (the only shape `*.context_length`
/// uses in practice) and, always, the position immediately after the
/// value — every branch advances correctly even for types the caller does
/// not care about, so scanning can skip past unrelated keys.
fn read_value(buf: &[u8], pos: usize, ty: GgufValueType) -> Option<(Option<u64>, usize)> {
    match ty {
        GgufValueType::U8 | GgufValueType::I8 | GgufValueType::Bool => {
            let end = pos.checked_add(1)?;
            if end > buf.len() {
                return None;
            }
            Some((Some(buf[pos] as u64), end))
        }
        GgufValueType::U16 | GgufValueType::I16 => {
            let end = pos.checked_add(2)?;
            if end > buf.len() {
                return None;
            }
            let v = u16::from_le_bytes(buf[pos..end].try_into().ok()?);
            Some((Some(v as u64), end))
        }
        GgufValueType::U32 | GgufValueType::I32 => {
            let end = pos.checked_add(4)?;
            if end > buf.len() {
                return None;
            }
            let v = u32::from_le_bytes(buf[pos..end].try_into().ok()?);
            Some((Some(v as u64), end))
        }
        GgufValueType::F32 => {
            let end = pos.checked_add(4)?;
            if end > buf.len() {
                return None;
            }
            Some((None, end))
        }
        GgufValueType::U64 | GgufValueType::I64 => {
            let end = pos.checked_add(8)?;
            if end > buf.len() {
                return None;
            }
            let v = u64::from_le_bytes(buf[pos..end].try_into().ok()?);
            Some((Some(v), end))
        }
        GgufValueType::F64 => {
            let end = pos.checked_add(8)?;
            if end > buf.len() {
                return None;
            }
            Some((None, end))
        }
        GgufValueType::String => {
            let (_s, next) = read_gguf_string(buf, pos)?;
            Some((None, next))
        }
        GgufValueType::Array => {
            let elem_ty = value_type_from_u32(read_u32(buf, pos)?)?;
            let count = read_u64(buf, pos.checked_add(4)?)?;
            let mut cursor = pos.checked_add(12)?;
            for _ in 0..count {
                let (_, next) = read_value(buf, cursor, elem_ty)?;
                cursor = next;
            }
            Some((None, cursor))
        }
    }
}

/// Best-effort extraction of `*.context_length` from a GGUF header. Only
/// invoked as the Entscheidung-9 safety fallback when quantization/size
/// metadata otherwise leaves a `TooBig` decision ambiguous — the caller
/// fetches just the first few KiB via a Range request. Returns `None` on
/// any malformed or truncated input rather than erroring: an unreadable
/// header degrades to `metadataProvenance: unknown`, never blocks the rest
/// of the preview.
pub fn parse_gguf_context_length(buf: &[u8]) -> Option<u64> {
    if buf.len() < 4 || &buf[0..4] != b"GGUF" {
        return None;
    }
    let version = read_u32(buf, 4)?;
    if version < 2 {
        // v1 used 32-bit counts and is long obsolete; not supported.
        return None;
    }
    let _tensor_count = read_u64(buf, 8)?;
    let kv_count = read_u64(buf, 16)?;
    let mut pos = 24usize;
    for _ in 0..kv_count {
        let (key, next) = read_gguf_string(buf, pos)?;
        pos = next;
        let ty = value_type_from_u32(read_u32(buf, pos)?)?;
        pos = pos.checked_add(4)?;
        let (numeric, next) = read_value(buf, pos, ty)?;
        pos = next;
        if key.ends_with(".context_length") {
            if let Some(n) = numeric {
                return Some(n);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------
// HTTP transport boundary
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawSibling {
    rfilename: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawCardData {
    #[serde(default)]
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawModelInfo {
    id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    gated: Value,
    #[serde(default)]
    private: Option<bool>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    card_data: Option<RawCardData>,
    #[serde(default)]
    siblings: Option<Vec<RawSibling>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLfsInfo {
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawTreeEntry {
    path: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<RawLfsInfo>,
}

fn is_gated(v: &Value) -> bool {
    !matches!(v, Value::Bool(false) | Value::Null)
}

fn extract_license(info: &RawModelInfo) -> Option<String> {
    if let Some(license) = info.card_data.as_ref().and_then(|cd| cd.license.clone()) {
        if !license.is_empty() {
            return Some(license);
        }
    }
    info.tags
        .as_ref()?
        .iter()
        .find_map(|t| t.strip_prefix("license:").map(|s| s.to_string()))
}

fn map_reqwest_err(e: reqwest::Error) -> HolziError {
    if e.is_timeout() {
        HolziError::Timeout {
            reason: e.to_string(),
        }
    } else {
        HolziError::Network {
            reason: e.to_string(),
        }
    }
}

async fn finish_response(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(HolziError::RateLimited {
            retry_after_seconds: retry_after,
        });
    }
    if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
        let body = resp.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(500).collect();
        return Err(HolziError::HttpStatus {
            status: status.as_u16(),
            reason: truncated,
        });
    }
    Ok(resp)
}

/// Builds `<base>/<prefix...>/<owner>/<name>/<extra...>` with every
/// segment percent-encoded by `Url::path_segments_mut`. `repo_id` is split
/// on its single validated `/` so `owner` and `name` land as two distinct
/// path segments rather than one segment containing an encoded slash.
fn build_path_url(base_url: &str, prefix: &[String], repo_id: &str, extra: &[&str]) -> Result<Url> {
    let mut url = Url::parse(base_url).map_err(|e| HolziError::InvalidInput {
        reason: format!("invalid base url: {e}"),
    })?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| HolziError::InvalidInput {
                reason: "base url cannot be a base".into(),
            })?;
        for p in prefix {
            if !p.is_empty() {
                segments.push(p);
            }
        }
        for part in repo_id.split('/') {
            segments.push(part);
        }
        for e in extra {
            segments.push(e);
        }
    }
    Ok(url)
}

/// Thin, test-overridable HTTP boundary around the public Hugging Face Hub
/// API. Every method validates its own inputs before building a URL.
pub struct HfClient {
    client: Client,
    base_url: String,
}

impl HfClient {
    /// `base_url` is `DEFAULT_BASE_URL` in production; tests pass a
    /// `wiremock` server URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HolziError::Network {
                reason: format!("build reqwest client: {e}"),
            })?;
        Ok(Self {
            client,
            base_url: base_url.into(),
        })
    }

    /// HuggingFace, or the e2e suite's stand-in server (`stand_in.rs`).
    pub fn production() -> Result<Self> {
        Self::new(super::stand_in::hf_base_url())
    }

    /// One page of the Hub's model list. `cursor` is the opaque token the Hub handed out with the
    /// previous page; the returned one (if any) continues the same list.
    async fn search(
        &self,
        query: Option<&str>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<RawModelInfo>, Option<String>)> {
        let mut url = Url::parse(&self.base_url).map_err(|e| HolziError::InvalidInput {
            reason: format!("invalid base url: {e}"),
        })?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| HolziError::InvalidInput {
                    reason: "base url cannot be a base".into(),
                })?;
            segments.push("api").push("models");
        }
        {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(query) = query {
                query_pairs.append_pair("search", query);
            }
            // Only GGUF repositories can be installed. Asking the Hub for them (instead of
            // dropping the rest afterwards) keeps pages full: the most downloaded models of all
            // are bert, gpt2 and the like, which would leave nothing of a page of ten.
            query_pairs.append_pair("filter", "gguf");
            if query.is_none() {
                // The query-less list is the chat-model discovery view. The Hub tags every repo
                // with a chat template `conversational`; `pipeline_tag=text-generation` would
                // drop the multimodal chat models (Gemma 4, Qwen3.8: `image-text-to-text`),
                // which are most of the top GGUF chat downloads. Several `filter`s must all hold.
                query_pairs.append_pair("filter", "conversational");
            }
            if let Some(cursor) = cursor {
                query_pairs.append_pair("cursor", cursor);
            }
            query_pairs
                .append_pair("limit", &limit.to_string())
                .append_pair("sort", "downloads")
                .append_pair("direction", "-1")
                .append_pair("full", "true");
        }
        let resp = self
            .client
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        let resp = finish_response(resp).await?;
        let next_cursor = next_cursor_from_link_header(
            resp.headers()
                .get(reqwest::header::LINK)
                .and_then(|v| v.to_str().ok()),
        );
        let hits = resp
            .json::<Vec<RawModelInfo>>()
            .await
            .map_err(|e| HolziError::Network {
                reason: format!("parse search response: {e}"),
            })?;
        Ok((hits, next_cursor))
    }

    async fn repo_info(&self, repo_id: &str, revision: Option<&str>) -> Result<RawModelInfo> {
        let extra: &[&str] = match revision {
            Some(r) => &["revision", r],
            None => &[],
        };
        let url = build_path_url(
            &self.base_url,
            &["api".to_string(), "models".to_string()],
            repo_id,
            extra,
        )?;
        let resp = self
            .client
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        let resp = finish_response(resp).await?;
        resp.json::<RawModelInfo>()
            .await
            .map_err(|e| HolziError::Network {
                reason: format!("parse repo info response: {e}"),
            })
    }

    async fn tree(&self, repo_id: &str, revision_sha: &str) -> Result<Vec<RawTreeEntry>> {
        let url = build_path_url(
            &self.base_url,
            &["api".to_string(), "models".to_string()],
            repo_id,
            &["tree", revision_sha],
        )?;
        let resp = self
            .client
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        let resp = finish_response(resp).await?;
        resp.json::<Vec<RawTreeEntry>>()
            .await
            .map_err(|e| HolziError::Network {
                reason: format!("parse tree response: {e}"),
            })
    }

    /// Fetches the first `max_bytes` of a file via an HTTP Range request —
    /// used only for the Entscheidung-9 GGUF-header safety fallback, never
    /// for a full model download (that always goes through
    /// `download::download_to_file` against [`resolve_download_url`]).
    pub async fn fetch_header_bytes(
        &self,
        repo_id: &str,
        revision_sha: &str,
        filename: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        let url = build_path_url(
            &self.base_url,
            &["".to_string()],
            repo_id,
            &["resolve", revision_sha, filename],
        )?;
        let resp = self
            .client
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .header(
                reqwest::header::RANGE,
                format!("bytes=0-{}", max_bytes.saturating_sub(1)),
            )
            .send()
            .await
            .map_err(map_reqwest_err)?;
        let resp = finish_response(resp).await?;
        if resp.status() != StatusCode::PARTIAL_CONTENT {
            return Err(HolziError::HttpStatus {
                status: resp.status().as_u16(),
                reason: "range request must return HTTP 206 Partial Content".into(),
            });
        }
        if resp
            .content_length()
            .is_some_and(|length| length > max_bytes)
        {
            return Err(HolziError::HttpStatus {
                status: resp.status().as_u16(),
                reason: format!("range response exceeds the {max_bytes}-byte header limit"),
            });
        }

        let mut stream = resp.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_reqwest_err)?;
            let remaining = max_bytes.saturating_sub(bytes.len() as u64);
            if remaining == 0 {
                break;
            }
            let take = remaining.min(chunk.len() as u64) as usize;
            bytes.extend_from_slice(&chunk[..take]);
            if take < chunk.len() {
                break;
            }
        }
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------
// Orchestration (used directly by models::commands)
// ---------------------------------------------------------------------

fn validate_search_query(query: Option<&str>) -> Result<Option<String>> {
    let Some(query) = query else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.chars().count() < 2 {
        return Err(HolziError::InvalidInput {
            reason: "search query must be at least 2 characters".into(),
        });
    }
    if trimmed.len() > 200 {
        return Err(HolziError::InvalidInput {
            reason: "search query is too long".into(),
        });
    }
    Ok(Some(trimmed.to_string()))
}

/// The `cursor` of the `rel="next"` entry of the Hub's `Link` header, the way the Hub paginates
/// its model list (`<https://…/api/models?…&cursor=…>; rel="next"`).
fn next_cursor_from_link_header(link: Option<&str>) -> Option<String> {
    link?
        .split(',')
        .filter(|entry| entry.contains("rel=\"next\""))
        .find_map(|entry| {
            let target = entry.split(';').next()?.trim().strip_prefix('<')?;
            let url = Url::parse(target.strip_suffix('>')?).ok()?;
            url.query_pairs()
                .find(|(key, _)| key == "cursor")
                .map(|(_, value)| value.into_owned())
        })
}

fn normalize_search_hit(hit: RawModelInfo) -> Option<HuggingFaceModelResult> {
    if hit.private.unwrap_or(false) || is_gated(&hit.gated) {
        return None;
    }
    if validate_repo_id(&hit.id).is_err() {
        return None;
    }
    let sha = hit.sha.clone()?;
    let sibling_names: Vec<String> = hit
        .siblings
        .as_ref()
        .map(|s| s.iter().map(|s| s.rfilename.clone()).collect())
        .unwrap_or_default();
    let gguf_names: Vec<String> = sibling_names
        .iter()
        .filter(|f| is_gguf_filename(f))
        .cloned()
        .collect();
    if gguf_names.is_empty() {
        return None;
    }
    let (tokenizer_repo, tokenizer_required) = resolve_tokenizer_repo(&hit.id, &sibling_names);
    let license = extract_license(&hit);
    let files = gguf_names
        .into_iter()
        .map(|filename| {
            let quantization = normalize_quantization_from_filename(&filename);
            let (catalog_match, catalog_entry_id) = catalog_match_for(&hit.id, &filename);
            HuggingFaceFileCandidate {
                repo_id: hit.id.clone(),
                filename,
                revision: sha.clone(),
                revision_ref: None,
                size_bytes: None,
                metadata_provenance: MetadataProvenance {
                    quantization: if quantization.is_some() {
                        MetadataProvenanceSource::FilenameHeuristic
                    } else {
                        MetadataProvenanceSource::Unknown
                    },
                    context_window: MetadataProvenanceSource::Unknown,
                },
                quantization,
                context_window: None,
                tokenizer_repo: tokenizer_repo.clone(),
                tokenizer_required,
                fit: Fit::Unknown,
                catalog_match,
                catalog_entry_id,
            }
        })
        .collect();
    Some(HuggingFaceModelResult {
        repo_id: hit.id.clone(),
        display_name: hit.id.clone(),
        author: hit.author,
        license,
        downloads: hit.downloads,
        files,
        source_revision: Some(sha),
        revision_ref: None,
    })
}

pub const DEFAULT_TOP_LIMIT: usize = 10;

/// One page of search hits and the cursor of the next page, if there is one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuggingFaceSearchPage {
    pub results: Vec<HuggingFaceModelResult>,
    pub next_cursor: Option<String>,
}

/// `search_huggingface_models`: validates an optional query, fetches one page
/// from the Hub, normalizes every hit, then deduplicates and deterministically
/// sorts the page. Without a query the Hub's download sort supplies the default
/// top-model view of chat models, ten per page; explicit searches page by twenty.
/// `cursor` continues a list with the `next_cursor` of the previous page.
pub async fn search_models(
    hf: &HfClient,
    query: Option<&str>,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<HuggingFaceSearchPage> {
    let query = validate_search_query(query)?;
    // The cap doubles as the default, so an explicit search gets twenty
    // results and the query-less discovery view ten.
    let max_limit = if query.is_some() {
        DEFAULT_LIMIT
    } else {
        DEFAULT_TOP_LIMIT
    };
    let capped = limit.unwrap_or(max_limit).clamp(1, max_limit);
    let (hits, next_cursor) = hf.search(query.as_deref(), capped, cursor).await?;
    let mut results: Vec<HuggingFaceModelResult> =
        hits.into_iter().filter_map(normalize_search_hit).collect();
    dedupe_and_sort(&mut results);
    results.truncate(capped);
    Ok(HuggingFaceSearchPage {
        results,
        next_cursor,
    })
}

/// `get_huggingface_model_details`: fetches repo metadata (resolving
/// `revision` to a commit SHA in the same call) plus the file tree for
/// real byte sizes, then normalizes every GGUF file candidate.
///
/// The tree call is best-effort: a failure there degrades every file's
/// `sizeBytes`/`fit` to `None`/`Unknown` rather than failing the whole
/// details call — file listing and installability do not depend on it.
pub async fn get_model_details(
    hf: &HfClient,
    hw: &HardwareInfo,
    repo_id: &str,
    revision: Option<&str>,
) -> Result<HuggingFaceModelResult> {
    validate_repo_id(repo_id)?;
    if let Some(r) = revision {
        validate_revision_ref(r)?;
    }
    let info = hf.repo_info(repo_id, revision).await?;
    if info.private.unwrap_or(false) || is_gated(&info.gated) {
        return Err(HolziError::InvalidInput {
            reason: format!("{repo_id} is private or gated"),
        });
    }
    let sha = info.sha.clone().ok_or_else(|| HolziError::InvalidInput {
        reason: format!("could not resolve a revision for {repo_id}"),
    })?;
    let sibling_names: Vec<String> = info
        .siblings
        .as_ref()
        .map(|s| s.iter().map(|s| s.rfilename.clone()).collect())
        .unwrap_or_default();
    let gguf_names: Vec<String> = sibling_names
        .iter()
        .filter(|f| is_gguf_filename(f))
        .cloned()
        .collect();
    if gguf_names.is_empty() {
        return Err(HolziError::UnsupportedFormat {
            filename: repo_id.to_string(),
        });
    }
    let sizes: HashMap<String, u64> = hf
        .tree(repo_id, &sha)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| {
            let size = e.lfs.and_then(|l| l.size).or(e.size)?;
            Some((e.path, size))
        })
        .collect();
    let (tokenizer_repo, tokenizer_required) = resolve_tokenizer_repo(repo_id, &sibling_names);
    let license = extract_license(&info);
    let revision_ref = revision
        .map(|r| r.to_string())
        .filter(|r| !is_commit_sha(r));
    let files = gguf_names
        .into_iter()
        .map(|filename| {
            let size_bytes = sizes.get(&filename).copied();
            let quantization = normalize_quantization_from_filename(&filename);
            let fit = size_bytes
                .map(|sz| {
                    classify(
                        hw,
                        ModelFitInputs {
                            file_size_bytes: sz,
                            context_window: None,
                        },
                    )
                })
                .unwrap_or(Fit::Unknown);
            let (catalog_match, catalog_entry_id) = catalog_match_for(repo_id, &filename);
            HuggingFaceFileCandidate {
                repo_id: repo_id.to_string(),
                filename,
                revision: sha.clone(),
                revision_ref: revision_ref.clone(),
                size_bytes,
                metadata_provenance: MetadataProvenance {
                    quantization: if quantization.is_some() {
                        MetadataProvenanceSource::FilenameHeuristic
                    } else {
                        MetadataProvenanceSource::Unknown
                    },
                    context_window: MetadataProvenanceSource::Unknown,
                },
                quantization,
                context_window: None,
                tokenizer_repo: tokenizer_repo.clone(),
                tokenizer_required,
                fit,
                catalog_match,
                catalog_entry_id,
            }
        })
        .collect();
    Ok(HuggingFaceModelResult {
        repo_id: repo_id.to_string(),
        display_name: repo_id.to_string(),
        author: info.author,
        license,
        downloads: info.downloads,
        files,
        source_revision: Some(sha),
        revision_ref,
    })
}

/// Resolves an optional user-supplied revision (branch, tag, or SHA) to a
/// concrete commit SHA (research.md Entscheidung 7). `revision_ref` is set
/// to the tracked branch/tag name for anything other than a direct SHA
/// pin — including the implicit `main` default — so
/// `check_huggingface_model_updates` has something to compare later.
pub async fn resolve_revision(
    hf: &HfClient,
    repo_id: &str,
    revision: Option<&str>,
) -> Result<RevisionResolution> {
    validate_repo_id(repo_id)?;
    if let Some(r) = revision {
        validate_revision_ref(r)?;
        if is_commit_sha(r) {
            let info = hf.repo_info(repo_id, Some(r)).await?;
            let sha = info.sha.unwrap_or_else(|| r.to_string());
            return Ok(RevisionResolution {
                sha,
                revision_ref: None,
            });
        }
        let info = hf.repo_info(repo_id, Some(r)).await?;
        let sha = info.sha.ok_or_else(|| HolziError::InvalidInput {
            reason: format!("could not resolve revision '{r}' for {repo_id}"),
        })?;
        return Ok(RevisionResolution {
            sha,
            revision_ref: Some(r.to_string()),
        });
    }
    let info = hf.repo_info(repo_id, None).await?;
    let sha = info.sha.ok_or_else(|| HolziError::InvalidInput {
        reason: format!("could not resolve the default revision for {repo_id}"),
    })?;
    Ok(RevisionResolution {
        sha,
        revision_ref: Some(DEFAULT_REVISION_REF.to_string()),
    })
}

/// `preview_huggingface_install`: resolves the revision, looks up the
/// file's real size (best-effort) and tokenizer, and classifies hardware
/// fit — without downloading or persisting anything.
#[allow(clippy::too_many_arguments)]
pub async fn preview_install(
    hf: &HfClient,
    hw: &HardwareInfo,
    repo_id: &str,
    filename: &str,
    revision: Option<&str>,
    tokenizer_repo_override: Option<&str>,
    context_window_override: Option<u64>,
) -> Result<InstallPreview> {
    validate_repo_id(repo_id)?;
    validate_gguf_filename(filename)?;
    if let Some(tokenizer_repo) = tokenizer_repo_override {
        validate_repo_id(tokenizer_repo)?;
    }
    let resolved = resolve_revision(hf, repo_id, revision).await?;
    let info = hf.repo_info(repo_id, Some(&resolved.sha)).await?;
    let sibling_names: Vec<String> = info
        .siblings
        .as_ref()
        .map(|s| s.iter().map(|s| s.rfilename.clone()).collect())
        .unwrap_or_default();
    if !sibling_names.iter().any(|s| s == filename) {
        return Err(HolziError::InvalidInput {
            reason: format!("{filename} not found in {repo_id}@{}", resolved.sha),
        });
    }
    let size_bytes = hf
        .tree(repo_id, &resolved.sha)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|e| e.path == filename)
        .and_then(|e| e.lfs.and_then(|l| l.size).or(e.size));
    let (auto_tokenizer_repo, _) = resolve_tokenizer_repo(repo_id, &sibling_names);
    let tokenizer_repo = tokenizer_repo_override
        .map(|s| s.to_string())
        .or(auto_tokenizer_repo);
    let tokenizer_required = tokenizer_repo.is_none();
    let quantization = normalize_quantization_from_filename(filename);
    let context_window = context_window_override;
    let fit = size_bytes
        .map(|sz| {
            classify(
                hw,
                ModelFitInputs {
                    file_size_bytes: sz,
                    context_window,
                },
            )
        })
        .unwrap_or(Fit::Unknown);
    let (catalog_match, catalog_entry_id) = catalog_match_for(repo_id, filename);
    Ok(InstallPreview {
        model_id: derive_model_id(repo_id, filename),
        name: filename.to_string(),
        repo_id: repo_id.to_string(),
        filename: filename.to_string(),
        revision: resolved.sha,
        revision_ref: resolved.revision_ref,
        size_bytes,
        metadata_provenance: MetadataProvenance {
            quantization: if quantization.is_some() {
                MetadataProvenanceSource::FilenameHeuristic
            } else {
                MetadataProvenanceSource::Unknown
            },
            // This value is supplied by the caller, not read from Hub
            // metadata; it therefore has no stronger provenance than unknown.
            context_window: MetadataProvenanceSource::Unknown,
        },
        quantization,
        context_window,
        tokenizer_repo,
        tokenizer_required,
        catalog_match,
        catalog_entry_id,
        fit,
        requires_explicit_too_big_confirmation: matches!(fit, Fit::TooBig),
    })
}

/// Looks up a single file's real byte size from the repo's file tree at a
/// resolved commit SHA. Used as a pre-flight, bandwidth-cheap check before
/// a download that must enforce the `TooBig` confirmation gate — reusing
/// [`HfClient::tree`] instead of duplicating the tree call/parse.
pub async fn lookup_file_size(
    hf: &HfClient,
    repo_id: &str,
    revision_sha: &str,
    filename: &str,
) -> Result<Option<u64>> {
    validate_repo_id(repo_id)?;
    let entries = hf.tree(repo_id, revision_sha).await?;
    Ok(entries
        .into_iter()
        .find(|e| e.path == filename)
        .and_then(|e| e.lfs.and_then(|l| l.size).or(e.size)))
}

/// `check_huggingface_model_updates`: resolves the tracked `revision_ref`
/// to its current commit SHA. Never touches local state — the caller
/// compares against the stored `installedRevision`.
pub async fn check_update(hf: &HfClient, repo_id: &str, revision_ref: &str) -> Result<String> {
    validate_repo_id(repo_id)?;
    validate_revision_ref(revision_ref)?;
    let info = hf.repo_info(repo_id, Some(revision_ref)).await?;
    info.sha.ok_or_else(|| HolziError::InvalidInput {
        reason: format!("could not resolve '{revision_ref}' for {repo_id}"),
    })
}

#[cfg(test)]
#[path = "huggingface_tests.rs"]
mod tests;
