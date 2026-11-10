//! Minimal hardware detection used to filter the model catalog.
//!
//! The plan (§"Lokale Inferenz: Runtime und Modellbezug") is explicit:
//! hardware info is **information, not a gate**. The filter here labels
//! catalog entries with `Fit::Fits` / `Fit::Tight` / `Fit::TooBig` so
//! the UI can render them grouped — a load attempt with insufficient
//! memory still runs, and the failure surfaces from llama.cpp directly.
//!
//! Backend detection is compile-time (whatever llama.cpp backend was built
//! with). RAM comes from `sysinfo`. VRAM is best-effort: for CUDA hosts
//! we shell out to `nvidia-smi` (already present when CUDA is), for
//! Metal we return `None` and rely on unified memory ≈ RAM, for CPU we
//! return `None` because there is no separate device.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sysinfo::System;

mod fit;
#[cfg(test)]
mod fit_tests;
pub mod hostname;
pub mod tiers;
#[cfg(test)]
mod tiers_tests;

pub use fit::{classify, Fit, ModelFitInputs};

/// Tauri command: current hardware snapshot. Called once at onboarding
/// to size the model suggestions, and on demand from Settings.
#[tauri::command]
pub async fn get_hardware_info() -> HardwareInfo {
    probe_async().await
}

/// [`probe`] off the async executor. Every caller inside an `async fn`
/// must use this: `probe` reads `/proc` and, on CUDA builds, spawns
/// `nvidia-smi` and polls it with a blocking sleep for up to
/// [`CUDA_PROBE_TIMEOUT`], which would stall a Tokio worker thread.
///
/// Falls back to an inline probe when the blocking pool cannot run the
/// task — a hardware snapshot is advisory, so a join failure must not
/// fail the command that needed it.
pub async fn probe_async() -> HardwareInfo {
    match tauri::async_runtime::spawn_blocking(probe).await {
        Ok(info) => info,
        Err(_) => probe(),
    }
}

/// Snapshot of the host relevant to the model catalog UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub backend: Backend,
    pub total_ram_bytes: u64,
    pub available_ram_bytes: u64,
    /// Only populated on CUDA hosts where `nvidia-smi` returned a
    /// value; `None` on CPU/Metal or when the probe failed.
    pub vram_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cpu,
    Cuda,
    Metal,
}

impl Backend {
    pub const fn from_features() -> Self {
        if cfg!(feature = "llm-llamacpp-cuda") {
            Backend::Cuda
        } else if cfg!(feature = "llm-llamacpp-metal") {
            Backend::Metal
        } else {
            Backend::Cpu
        }
    }
}

/// Probes the host. Blocking: reads `/proc` via sysinfo and, on CUDA
/// hosts, spawns `nvidia-smi` and polls it for up to
/// [`CUDA_PROBE_TIMEOUT`]. Call [`probe_async`] from `async` code.
pub fn probe() -> HardwareInfo {
    let mut sys = System::new();
    sys.refresh_memory();
    let total_ram_bytes = sys.total_memory();
    let available_ram_bytes = sys.available_memory();

    let backend = Backend::from_features();
    let vram_bytes = match backend {
        Backend::Cuda if crate::platform::capabilities().gpu_detection => probe_cuda_vram(),
        Backend::Cuda | Backend::Metal | Backend::Cpu => None,
    };

    HardwareInfo {
        backend,
        total_ram_bytes,
        available_ram_bytes,
        vram_bytes,
    }
}

/// Best-effort VRAM detection via `nvidia-smi`. Runs synchronously with
/// a hard cap of 500 ms; any failure (missing binary, non-zero exit,
/// unparseable output) returns `None` — VRAM is informational.
fn probe_cuda_vram() -> Option<u64> {
    // Spawn nvidia-smi directly so the probe works on Windows and macOS
    // without requiring the GNU `timeout` executable. Enforce the same
    // deadline on every platform in Rust.
    let mut child = Command::new("nvidia-smi")
        .arg("--query-gpu=memory.total")
        .arg("--format=csv,noheader,nounits")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + CUDA_PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };

    if !status.success() {
        return None;
    }

    let mut stdout = child.stdout.take()?;
    let mut output = Vec::new();
    stdout.read_to_end(&mut output).ok()?;

    let stdout = String::from_utf8(output).ok()?;
    // First line, first GPU. Value is in MiB per the format spec.
    let mib: u64 = stdout.lines().next()?.trim().parse().ok()?;
    Some(mib * 1024 * 1024)
}

/// The wall-clock cap on the CUDA VRAM probe. Exported for tests.
#[allow(dead_code)]
pub const CUDA_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
