//! The platform key of a device: the name under which a vendor's artifact for this operating
//! system and CPU architecture is listed in the catalog (spec 094).

use std::fmt;

/// An operating system and CPU architecture a delegate tool is released for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformKey {
    LinuxX64,
    LinuxArm64,
    DarwinX64,
    DarwinArm64,
    Win32X64,
    Win32Arm64,
}

impl PlatformKey {
    /// Every key, in the order the catalog lists them.
    pub const ALL: [PlatformKey; 6] = [
        PlatformKey::LinuxX64,
        PlatformKey::LinuxArm64,
        PlatformKey::DarwinX64,
        PlatformKey::DarwinArm64,
        PlatformKey::Win32X64,
        PlatformKey::Win32Arm64,
    ];

    /// The key of the device this runs on, or `None` for a combination no vendor releases for
    /// (Android and iOS never reach here: the tool manager is desktop only, spec FR-016).
    pub fn host() -> Option<Self> {
        Self::from_parts(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// The key for the operating system and architecture names of `std::env::consts`.
    pub fn from_parts(os: &str, arch: &str) -> Option<Self> {
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

impl fmt::Display for PlatformKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
