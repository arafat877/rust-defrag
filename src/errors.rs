/// errors.rs — Centralized error definitions for RustDefrag
///
/// Uses `anyhow` for flexible error propagation and `thiserror`-style
/// custom variants for domain-specific failures.

use std::fmt;

/// All domain-specific errors that RustDefrag can produce.
#[derive(Debug)]
#[allow(dead_code)]
pub enum DefragError {
    /// Caller is not running as Administrator.
    InsufficientPrivileges,

    /// The given drive letter is invalid or cannot be opened.
    InvalidVolume(String),

    /// The volume uses a filesystem we do not support (e.g. FAT32).
    UnsupportedFilesystem(String),

    /// The volume is exclusively locked by another process.
    VolumeLocked,

    /// A Windows API call returned an unexpected error code.
    ApiFailure { api: &'static str, code: u32 },

    /// A file could not be opened (e.g. access denied, sharing violation).
    FileAccessDenied(String),

    /// FSCTL_MOVE_FILE failed for a specific file.
    MoveFileFailed { path: String, code: u32 },

    /// No contiguous free cluster region large enough was found.
    NoFreeRegion { required: u64 },

    /// An I/O error from the standard library.
    Io(std::io::Error),
}

impl fmt::Display for DefragError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefragError::InsufficientPrivileges => {
                write!(f, "Administrator privileges are required. Please re-run as Administrator.")
            }
            DefragError::InvalidVolume(v) => {
                write!(f, "Cannot open volume '{}'. Ensure the drive letter is correct.", v)
            }
            DefragError::UnsupportedFilesystem(fs) => {
                write!(f, "Filesystem '{}' is not supported. RustDefrag requires NTFS.", fs)
            }
            DefragError::VolumeLocked => {
                write!(f, "The volume is locked by another process. Close any programs using the disk and retry.")
            }
            DefragError::ApiFailure { api, code } => {
                write!(f, "Windows API '{}' failed with error code 0x{:08X}.", api, code)
            }
            DefragError::FileAccessDenied(path) => {
                write!(f, "Access denied when opening file: '{}'.", path)
            }
            DefragError::MoveFileFailed { path, code } => {
                write!(
                    f,
                    "Failed to move clusters for '{}' (error 0x{:08X}). File will be skipped.",
                    path, code
                )
            }
            DefragError::NoFreeRegion { required } => {
                write!(
                    f,
                    "Could not find a contiguous free region of {} clusters. Disk may be too full.",
                    required
                )
            }
            DefragError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for DefragError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let DefragError::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<std::io::Error> for DefragError {
    fn from(e: std::io::Error) -> Self {
        DefragError::Io(e)
    }
}

/// Convenience alias — every function in RustDefrag returns this.
pub type DefragResult<T> = Result<T, anyhow::Error>;

/// Helper: wrap a Windows error code into a [`DefragError::ApiFailure`].
#[allow(dead_code)]
pub fn api_error(api: &'static str, code: u32) -> anyhow::Error {
    anyhow::Error::new(DefragError::ApiFailure { api, code })
}
