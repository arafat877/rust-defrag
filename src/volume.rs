/// volume.rs — Volume management for RustDefrag
///
/// Responsible for:
///   - Opening the NTFS volume device
///   - Querying cluster size
///   - Fetching the volume bitmap
///   - Enumerating files for analysis

use crate::errors::{DefragError, DefragResult};
use crate::winapi::{self, VolumeHandle};
use log::debug;
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────────
//  Public types
// ─────────────────────────────────────────────────────────────────────────────

/// High-level summary of the volume geometry.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// Human-readable label, e.g. `C:`.
    pub label: String,

    /// Filesystem name, always `"NTFS"` for supported volumes.
    pub filesystem: String,

    /// Bytes per cluster.
    pub cluster_size: u64,

    /// Total clusters on the volume.
    pub total_clusters: i64,

    /// Free clusters (derived from bitmap).
    pub free_clusters: i64,
}

/// Represents the volume cluster usage bitmap.
///
/// Bit `n` is 1 if cluster `starting_lcn + n` is in use.
#[derive(Debug)]
pub struct VolumeBitmap {
    pub starting_lcn: i64,
    pub total_clusters: i64,
    /// Raw bytes; bit order is LSB-first within each byte.
    pub bytes: Vec<u8>,
}

impl VolumeBitmap {
    /// Returns `true` if the cluster at logical cluster number `lcn` is in use.
    pub fn is_used(&self, lcn: i64) -> bool {
        let offset = lcn - self.starting_lcn;
        if offset < 0 || offset >= self.total_clusters {
            return true; // treat out-of-range as used (safe default)
        }
        let byte_idx = (offset / 8) as usize;
        let bit_idx = (offset % 8) as u8;
        (self.bytes[byte_idx] >> bit_idx) & 1 == 1
    }

    /// Returns `true` if cluster `lcn` is free.
    #[inline]
    pub fn is_free(&self, lcn: i64) -> bool {
        !self.is_used(lcn)
    }

    /// Count free clusters.
    pub fn free_count(&self) -> i64 {
        let mut free = 0i64;
        for byte in &self.bytes {
            free += byte.count_zeros() as i64;
        }
        // The last byte may contain extra padding bits; clamp.
        free.min(self.total_clusters)
    }

    /// Find the first contiguous run of `length` free clusters at or after `hint`.
    ///
    /// Returns `Some(lcn)` or `None` if no such region exists.
    pub fn find_free_run(&self, length: i64, hint: i64) -> Option<i64> {
        let start = (hint - self.starting_lcn).max(0);
        let end = self.total_clusters;

        let mut run_start = -1i64;
        let mut run_len = 0i64;

        let mut cluster = start;
        while cluster < end {
            if self.is_free(cluster + self.starting_lcn) {
                if run_start < 0 {
                    run_start = cluster;
                }
                run_len += 1;
                if run_len >= length {
                    return Some(run_start + self.starting_lcn);
                }
            } else {
                run_start = -1;
                run_len = 0;
            }
            cluster += 1;
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Volume open + info
// ─────────────────────────────────────────────────────────────────────────────

/// Open the target volume and return an owned handle + metadata.
pub fn open_volume(device_path: &str, drive_label: &str) -> DefragResult<(VolumeHandle, VolumeInfo)> {
    // Check filesystem type *before* opening with write access
    let filesystem = winapi::get_filesystem_type(drive_label)?;
    if !filesystem.eq_ignore_ascii_case("NTFS") {
        anyhow::bail!(DefragError::UnsupportedFilesystem(filesystem));
    }

    let handle = winapi::open_volume(device_path)?;

    // Query cluster geometry
    let cluster_size = get_cluster_size(drive_label)?;

    // Load the bitmap to count free clusters
    let (starting_lcn, total_clusters, bitmap_bytes) = winapi::get_volume_bitmap(&handle)?;
    let bitmap = VolumeBitmap {
        starting_lcn,
        total_clusters,
        bytes: bitmap_bytes,
    };
    let free_clusters = bitmap.free_count();

    let info = VolumeInfo {
        label: drive_label.to_string(),
        filesystem,
        cluster_size,
        total_clusters,
        free_clusters,
    };

    debug!("Volume opened: {:?}", info);
    Ok((handle, info))
}

/// Load just the bitmap (for analysis and defrag phases).
pub fn load_bitmap(handle: &VolumeHandle) -> DefragResult<VolumeBitmap> {
    let (starting_lcn, total_clusters, bytes) = winapi::get_volume_bitmap(handle)?;
    Ok(VolumeBitmap {
        starting_lcn,
        total_clusters,
        bytes,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  Cluster size
// ─────────────────────────────────────────────────────────────────────────────

/// Query the bytes-per-cluster for the given drive root.
pub fn get_cluster_size(drive_label: &str) -> DefragResult<u64> {
    #[cfg(target_os = "windows")]
    {
        use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetDiskFreeSpaceW};

        let root = format!("{}\\", drive_label);
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        let mut sectors_per_cluster: u32 = 0;
        let mut bytes_per_sector: u32 = 0;
        let mut free_clusters: u32 = 0;
        let mut total_clusters: u32 = 0;

        unsafe {
            GetDiskFreeSpaceW(
                PCWSTR(wide.as_ptr()),
                Some(&mut sectors_per_cluster),
                Some(&mut bytes_per_sector),
                Some(&mut free_clusters),
                Some(&mut total_clusters),
            )
        }
        .map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure {
                api: "GetDiskFreeSpaceW",
                code,
            }
        })?;

        Ok(sectors_per_cluster as u64 * bytes_per_sector as u64)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(4096) // default 4 KiB cluster for testing
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  File enumeration
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata about a file collected during enumeration.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub is_system: bool,
    pub is_temp: bool,
}

/// Walk the filesystem tree rooted at `root_dir` and return all regular files.
///
/// Files marked as system or temporary are flagged but still returned so
/// the caller can choose to skip them.
#[allow(dead_code)]
pub fn enumerate_files(root_dir: &Path) -> DefragResult<Vec<FileEntry>> {
    enumerate_files_with_progress(root_dir, |_| {})
}

/// Walk the filesystem tree rooted at `root_dir` and return all regular files.
///
/// Calls `on_progress(total_files_seen)` each time a file entry is collected.
pub fn enumerate_files_with_progress<F>(
    root_dir: &Path,
    mut on_progress: F,
) -> DefragResult<Vec<FileEntry>>
where
    F: FnMut(usize),
{
    let mut entries = Vec::new();
    enumerate_recursive(root_dir, &mut entries, &mut on_progress);
    Ok(entries)
}

fn enumerate_recursive<F>(dir: &Path, out: &mut Vec<FileEntry>, on_progress: &mut F)
where
    F: FnMut(usize),
{
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            debug!("Cannot read dir {:?}: {}", dir, e);
            return;
        }
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            enumerate_recursive(&path, out, on_progress);
        } else if meta.is_file() {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();

            // Mark protected files
            let is_system = filename.starts_with('$')
                || filename == "pagefile.sys"
                || filename == "hiberfil.sys"
                || filename == "swapfile.sys";

            #[cfg(target_os = "windows")]
            let is_system = {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0004;
                is_system || (meta.file_attributes() & FILE_ATTRIBUTE_SYSTEM != 0)
            };

            #[cfg(target_os = "windows")]
            let is_temp = {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x0100;
                meta.file_attributes() & FILE_ATTRIBUTE_TEMPORARY != 0
            };
            #[cfg(not(target_os = "windows"))]
            let is_temp = false;

            out.push(FileEntry {
                path,
                size_bytes: meta.len(),
                is_system,
                is_temp,
            });
            on_progress(out.len());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bitmap(bytes: Vec<u8>, total: i64) -> VolumeBitmap {
        VolumeBitmap {
            starting_lcn: 0,
            total_clusters: total,
            bytes,
        }
    }

    #[test]
    fn test_is_used() {
        // byte 0b0000_0001 → cluster 0 is used, cluster 1 is free
        let bm = make_bitmap(vec![0b0000_0001], 8);
        assert!(bm.is_used(0));
        assert!(bm.is_free(1));
    }

    #[test]
    fn test_find_free_run_simple() {
        // All free except cluster 0
        let bm = make_bitmap(vec![0b0000_0001, 0x00], 16);
        let result = bm.find_free_run(4, 0);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_free_count() {
        let bm = make_bitmap(vec![0x00, 0xFF], 16);
        // First byte: all free (8), second byte: all used (0)
        assert_eq!(bm.free_count(), 8);
    }

    #[test]
    fn test_no_free_run_found() {
        // All used
        let bm = make_bitmap(vec![0xFF; 4], 32);
        let result = bm.find_free_run(1, 0);
        assert!(result.is_none());
    }
}
