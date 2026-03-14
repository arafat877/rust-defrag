/// defrag.rs — Defragmentation engine
///
/// Takes the list of fragmented files from the analyser and moves their
/// cluster runs into contiguous free regions using `FSCTL_MOVE_FILE`.
///
/// Safety guarantees implemented here:
///   - Never touch NTFS metadata files (`$MFT`, `$LogFile`, …)
///   - Skip pagefile.sys / hiberfil.sys / swapfile.sys
///   - Skip files whose open returns `ERROR_SHARING_VIOLATION`
///   - On any move failure: log, skip, continue — never abort

use crate::analyzer::FileFragInfo;
use crate::errors::{DefragError, DefragResult};
use crate::progress::ProgressReporter;
use crate::volume::VolumeBitmap;
use crate::winapi::{self, VolumeHandle};
use log::{debug, warn};

// ─────────────────────────────────────────────────────────────────────────────
//  Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Results from a complete defrag pass.
#[derive(Debug, Default)]
pub struct DefragStats {
    pub files_attempted: u64,
    pub files_defragged: u64,
    pub files_skipped: u64,
    pub files_in_use: u64,
    pub clusters_moved: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
//  Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Defragment all files in `targets` using the volume `handle`.
///
/// `bitmap` is updated in memory after each successful move so subsequent
/// files can use newly freed regions.
pub fn defragment(
    vol: &VolumeHandle,
    targets: &[FileFragInfo],
    bitmap: &mut VolumeBitmap,
    progress: &ProgressReporter,
    verbose: bool,
) -> DefragResult<DefragStats> {
    let mut stats = DefragStats::default();
    let total = targets.len() as u64;

    progress.set_defrag_total(total);

    for (idx, file_info) in targets.iter().enumerate() {
        progress.set_defrag_pos(idx as u64);

        // Final safety check — should already be filtered, but be defensive
        if is_protected(&file_info.path) {
            warn!("Skipping protected file: {:?}", file_info.path);
            stats.files_skipped += 1;
            continue;
        }

        stats.files_attempted += 1;

        match defrag_single_file(vol, file_info, bitmap, verbose) {
            Ok(clusters_moved) => {
                if clusters_moved > 0 {
                    stats.files_defragged += 1;
                    stats.clusters_moved += clusters_moved;
                    debug!("Defragged {:?}: {} clusters moved", file_info.path, clusters_moved);
                } else {
                    stats.files_skipped += 1;
                }
            }
            Err(e) => {
                if is_file_in_use_error(&e) {
                    stats.files_in_use += 1;
                    debug!("Skipping in-use file: {:?}", file_info.path);
                } else {
                    warn!("Could not defrag {:?}: {}", file_info.path, e);
                }
                stats.files_skipped += 1;
            }
        }
    }

    progress.finish_defrag();
    Ok(stats)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Per-file defrag
// ─────────────────────────────────────────────────────────────────────────────

/// Attempt to consolidate `file_info` into a single contiguous region.
///
/// Returns the number of clusters actually moved (0 if nothing was done).
fn defrag_single_file(
    vol: &VolumeHandle,
    file_info: &FileFragInfo,
    bitmap: &mut VolumeBitmap,
    verbose: bool,
) -> DefragResult<u64> {
    // Open the file with write-through access so FSCTL_MOVE_FILE can proceed
    let file_handle = match open_file_for_move(&file_info.path) {
        Ok(h) => h,
        Err(e) => {
            debug!("Cannot open for move {:?}: {}", file_info.path, e);
            return Ok(0); // skip silently
        }
    };

    let total_clusters: i64 = file_info.runs.iter().map(|r| r.length).sum();
    if total_clusters == 0 {
        close_handle(file_handle);
        return Ok(0);
    }

    // Find a large enough contiguous free region
    let target_lcn = match bitmap.find_free_run(total_clusters, 0) {
        Some(lcn) => lcn,
        None => {
            debug!(
                "No free run of {} clusters for {:?}",
                total_clusters, file_info.path
            );
            close_handle(file_handle);
            return Ok(0);
        }
    };

    // Move each run sequentially into [target_lcn … target_lcn + total_clusters)
    let mut dest_lcn = target_lcn;
    let mut clusters_moved = 0u64;

    for run in &file_info.runs {
        let result = winapi::move_file_clusters(
            vol,
            file_handle,
            run.vcn,
            dest_lcn,
            run.length as u32,
        );

        match result {
            Ok(()) => {
                if verbose {
                    debug!(
                        "  Moved VCN {} → LCN {} (len {})",
                        run.vcn, dest_lcn, run.length
                    );
                }
                clusters_moved += run.length as u64;

                // Update bitmap: mark source as free, dest as used
                mark_clusters(bitmap, run.lcn, run.length, false);
                mark_clusters(bitmap, dest_lcn, run.length, true);

                dest_lcn += run.length;
            }
            Err(e) => {
                if is_file_in_use_error(&e) {
                    close_handle(file_handle);
                    anyhow::bail!(DefragError::FileAccessDenied(
                        file_info.path.display().to_string()
                    ));
                }
                warn!("Move failed for run in {:?}: {}", file_info.path, e);
            }
        }
    }

    close_handle(file_handle);
    Ok(clusters_moved)
}

fn is_file_in_use_error(err: &anyhow::Error) -> bool {
    if let Some(defrag_err) = err.downcast_ref::<DefragError>() {
        return matches!(
            defrag_err,
            DefragError::FileAccessDenied(_)
                | DefragError::ApiFailure { code: 5, .. }
                | DefragError::ApiFailure { code: 32, .. }
        );
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────
//  Bitmap update helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Mark `count` clusters starting at `lcn` as used (`true`) or free (`false`).
fn mark_clusters(bitmap: &mut VolumeBitmap, starting_lcn: i64, count: i64, used: bool) {
    for i in 0..count {
        let lcn = starting_lcn + i;
        let offset = lcn - bitmap.starting_lcn;
        if offset < 0 || offset >= bitmap.total_clusters {
            continue;
        }
        let byte_idx = (offset / 8) as usize;
        let bit_idx = (offset % 8) as u8;
        if used {
            bitmap.bytes[byte_idx] |= 1 << bit_idx;
        } else {
            bitmap.bytes[byte_idx] &= !(1 << bit_idx);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Safety filter
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` for NTFS metadata files and runtime lock files that must
/// never be relocated.
fn is_protected(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(
        name.as_str(),
        "$mft"
            | "$mftmirr"
            | "$logfile"
            | "$volume"
            | "$attrdef"
            | "$bitmap"
            | "$boot"
            | "$badclus"
            | "$secure"
            | "$upcase"
            | "$extend"
            | "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
    )
}

// ─────────────────────────────────────────────────────────────────────────────
//  File handle helpers (platform-specific)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn open_file_for_move(path: &std::path::Path) -> DefragResult<isize> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GetLastError, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_NO_BUFFERING,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            },
        },
    };

    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_NO_BUFFERING,
            None,
        )
    }
    .map_err(|_| {
        let code = unsafe { GetLastError().0 };
        DefragError::ApiFailure {
            api: "CreateFileW",
            code,
        }
    })?;

    if handle == INVALID_HANDLE_VALUE {
        anyhow::bail!(DefragError::FileAccessDenied(
            path.display().to_string()
        ));
    }
    Ok(handle.0 as isize)
}

#[cfg(not(target_os = "windows"))]
fn open_file_for_move(_path: &std::path::Path) -> DefragResult<isize> {
    Ok(1)
}

#[cfg(target_os = "windows")]
fn close_handle(handle: isize) {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    unsafe {
        let _ = CloseHandle(HANDLE(handle as _));
    }
}

#[cfg(not(target_os = "windows"))]
fn close_handle(_handle: isize) {}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_protected_metadata() {
        assert!(is_protected(std::path::Path::new("$MFT")));
        assert!(is_protected(std::path::Path::new("$LogFile")));
        assert!(is_protected(std::path::Path::new("pagefile.sys")));
        assert!(is_protected(std::path::Path::new("hiberfil.sys")));
    }

    #[test]
    fn test_is_protected_normal_file() {
        assert!(!is_protected(std::path::Path::new("document.txt")));
        assert!(!is_protected(std::path::Path::new("myapp.exe")));
    }

    #[test]
    fn test_mark_clusters_used() {
        let mut bm = crate::volume::VolumeBitmap {
            starting_lcn: 0,
            total_clusters: 64,
            bytes: vec![0u8; 8],
        };
        mark_clusters(&mut bm, 0, 3, true);
        assert!(bm.is_used(0));
        assert!(bm.is_used(1));
        assert!(bm.is_used(2));
        assert!(bm.is_free(3));
    }

    #[test]
    fn test_mark_clusters_free() {
        let mut bm = crate::volume::VolumeBitmap {
            starting_lcn: 0,
            total_clusters: 64,
            bytes: vec![0xFFu8; 8],
        };
        mark_clusters(&mut bm, 0, 4, false);
        assert!(bm.is_free(0));
        assert!(bm.is_free(3));
        assert!(bm.is_used(4));
    }
}


