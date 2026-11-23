//! The platform key of a device: the name under which a vendor's artifact for this operating
//! system and CPU architecture is listed in the catalog (spec 094).

use std::fmt;

/// An operating system and CPU architecture a delegate tool is released for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformKey {
    LinuxX64,
    /// Linux on an x86-64 CPU without AVX2: a vendor that builds for such CPUs lists this key
    /// (spec 096); every other vendor keeps using `linux-x64`.
    LinuxX64Baseline,
    LinuxArm64,
    DarwinX64,
    DarwinArm64,
    Win32X64,
    Win32Arm64,
}

impl PlatformKey {
    /// Every key, in the order the catalog lists them.
    pub const ALL: [PlatformKey; 7] = [
        PlatformKey::LinuxX64,
        PlatformKey::LinuxX64Baseline,
        PlatformKey::LinuxArm64,
        PlatformKey::DarwinX64,
        PlatformKey::DarwinArm64,
        PlatformKey::Win32X64,
        PlatformKey::Win32Arm64,
    ];

    /// The key of the device this runs on, or `None` for a combination no vendor releases for
    /// (Android and iOS never reach here: the tool manager is desktop only, spec FR-016).
    pub fn host() -> Option<Self> {
        Self::host_with(host_has_avx2())
    }

    /// [`PlatformKey::host`] with the CPU's answer given, so a test needs no particular CPU: a
    /// Linux x86-64 host without AVX2 is `linux-x64-baseline`.
    pub fn host_with(avx2: bool) -> Option<Self> {
        Self::from_parts_with(std::env::consts::OS, std::env::consts::ARCH, avx2)
    }

    /// [`PlatformKey::from_parts_with`] for a CPU that has AVX2.
    pub fn from_parts(os: &str, arch: &str) -> Option<Self> {
        Self::from_parts_with(os, arch, true)
    }

    /// The key for the operating system and architecture names of `std::env::consts` and the
    /// CPU's AVX2 answer. Only Linux x86-64 has a variant; macOS and Windows keep their key and
    /// an artifact says for itself whether it needs AVX2.
    pub fn from_parts_with(os: &str, arch: &str, avx2: bool) -> Option<Self> {
        match (os, arch) {
            ("linux", "x86_64") if !avx2 => Some(Self::LinuxX64Baseline),
            _ => Self::base_from_parts(os, arch),
        }
    }

    /// The keys a program for this device may be listed under, the most specific first: the
    /// variant, then the key every vendor has.
    pub fn candidates(self) -> Vec<PlatformKey> {
        match self {
            Self::LinuxX64Baseline => vec![Self::LinuxX64Baseline, Self::LinuxX64],
            other => vec![other],
        }
    }

    fn base_from_parts(os: &str, arch: &str) -> Option<Self> {
        match (os, arch) {
            ("linux", "x86_64") => Some(Self::LinuxX64),
            ("linux", "aarch64") => Some(Self::LinuxArm64),
            ("macos", "x86_64") => Some(Self::DarwinX64),
            ("macos", "aarch64") => Some(Self::DarwinArm64),
            ("windows", "x86_64") => Some(Self::Win32X64),
            ("windows", "aarch64") => Some(Self::Win32Arm64),
            _ => None,
        }
    }

    /// The key as the catalog and `install.json` write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX64 => "linux-x64",
            Self::LinuxX64Baseline => "linux-x64-baseline",
            Self::LinuxArm64 => "linux-arm64",
            Self::DarwinX64 => "darwin-x64",
            Self::DarwinArm64 => "darwin-arm64",
            Self::Win32X64 => "win32-x64",
            Self::Win32Arm64 => "win32-arm64",
        }
    }

    /// The key written as `as_str` writes it; `None` for any other text.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.as_str() == text)
    }

    /// Whether programs of this platform are Windows programs (`.exe`, no executable bit).
    pub fn is_windows(self) -> bool {
        matches!(self, Self::Win32X64 | Self::Win32Arm64)
    }
}

/// Whether this CPU has AVX2; `true` where the question does not arise (no x86-64 CPU).
pub fn host_has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        true
    }
}

impl fmt::Display for PlatformKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
