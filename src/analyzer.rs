/// Author : Arafat BOUCHAFRA <arafat877@gmail.com>
/// analyzer.rs â€” Fragmentation analysis engine
///
/// Scans the volume, retrieves cluster run lists for every file,
/// and builds a [`FragmentationReport`] with per-file and aggregate stats.

use crate::errors::DefragResult;
use crate::volume::FileEntry;
use crate::winapi::{self, ClusterRun};
use log::debug;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Public types
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Per-file fragmentation data.
#[derive(Debug, Clone)]
pub struct FileFragInfo {
    pub path: PathBuf,
    /// Number of contiguous cluster runs.
    pub fragment_count: u32,
    /// The individual runs.
    pub runs: Vec<ClusterRun>,
    /// True if the file has > 1 run.
    pub is_fragmented: bool,
}

impl FileFragInfo {
    /// Total clusters used by this file.
    pub fn total_clusters(&self) -> i64 {
        self.runs.iter().map(|r| r.length).sum()
    }
}

/// Aggregate fragmentation report for the entire volume.
#[derive(Debug, Default)]
pub struct FragmentationReport {
    pub total_files: u64,
    pub fragmented_files: u64,
    pub total_fragments: u64,
    pub total_clusters_used: i64,
    /// File with the most fragments.
    pub worst_file: Option<FileFragInfo>,
    /// All fragmented files, sorted worst-first.
    pub fragmented: Vec<FileFragInfo>,
}

impl FragmentationReport {
    /// Average fragments per file (1.0 means no fragmentation).
    pub fn average_fragments(&self) -> f64 {
        if self.total_files == 0 {
            1.0
        } else {
            self.total_fragments as f64 / self.total_files as f64
        }
    }

    /// Percentage of files that are fragmented.
    pub fn fragmentation_percent(&self) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            self.fragmented_files as f64 / self.total_files as f64 * 100.0
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Core analysis
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Analyse all files in `entries` and return a `FragmentationReport`.
///
/// Progress updates are delivered via `on_progress(files_done, total)`.
pub fn analyse_files<F>(
    entries: &[FileEntry],
    verbose: bool,
    on_progress: F,
) -> DefragResult<FragmentationReport>
where
    F: Fn(usize, usize) + Send + Sync,
{
    let total = entries.len();
    let report = Arc::new(Mutex::new(FragmentationReport::default()));
    let done_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Parallel scan using Rayon
    let file_infos: Vec<Option<FileFragInfo>> = entries
        .par_iter()
        .map(|entry| {
            // Skip protected files
            if entry.is_system || entry.is_temp {
                debug!("Skipping protected: {:?}", entry.path);
                let n = done_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                on_progress(n, total);
                return None;
            }

            let info = analyse_single_file(entry, verbose);

            let n = done_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            on_progress(n, total);

            info
        })
        .collect();

    // Aggregate
    let mut rep = report.lock().unwrap();
    for opt_info in file_infos.into_iter().flatten() {
        rep.total_files += 1;
        rep.total_fragments += opt_info.fragment_count as u64;
        rep.total_clusters_used += opt_info.total_clusters();

        if opt_info.is_fragmented {
            rep.fragmented_files += 1;

            // Track worst file
            let is_worse = rep
                .worst_file
                .as_ref()
                .map(|w| opt_info.fragment_count > w.fragment_count)
                .unwrap_or(true);
            if is_worse {
                rep.worst_file = Some(opt_info.clone());
            }

            rep.fragmented.push(opt_info);
        }
    }

    // Prioritize largest fragmented files first to maximize impact per move.
    // Tie-break by fragment count so heavily split files are still preferred.
    rep.fragmented.sort_unstable_by(|a, b| {
        b.total_clusters()
            .cmp(&a.total_clusters())
            .then_with(|| b.fragment_count.cmp(&a.fragment_count))
    });

    Ok(std::mem::take(&mut *rep))
}

/// Analyse a single file and return its fragmentation info.
///
/// Returns `None` if the file cannot be opened (access denied, locked, etc.).
fn analyse_single_file(entry: &FileEntry, verbose: bool) -> Option<FileFragInfo> {
    let handle = match open_file_for_query(&entry.path) {
        Ok(h) => h,
        Err(e) => {
            debug!("Cannot open {:?}: {}", entry.path, e);
            return None;
        }
    };

    let runs = match winapi::get_retrieval_pointers(handle) {
        Ok(r) => r,
        Err(e) => {
            debug!("get_retrieval_pointers failed for {:?}: {}", entry.path, e);
            close_file_handle(handle);
            return None;
        }
    };
    close_file_handle(handle);

    if runs.is_empty() {
        return None; // 0-byte or resident file
    }

    let fragment_count = runs.len() as u32;
    let is_fragmented = fragment_count > 1;

    if verbose && is_fragmented {
        debug!(
            "Fragmented: {:?}  fragments={}  size={}B",
            entry.path, fragment_count, entry.size_bytes
        );
    }

    Some(FileFragInfo {
        path: entry.path.clone(),
        fragment_count,
        runs,
        is_fragmented,
    })
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  File handle helpers (platform-specific)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(target_os = "windows")]
fn open_file_for_query(path: &std::path::Path) -> DefragResult<isize> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
                FILE_SHARE_WRITE, OPEN_EXISTING,
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
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS, // needed to open directories too
            None,
        )
    }
    .map_err(|e| anyhow::anyhow!("CreateFileW: {}", e))?;

    if handle == INVALID_HANDLE_VALUE {
        anyhow::bail!("INVALID_HANDLE_VALUE for {:?}", path);
    }
    Ok(handle.0 as isize)
}

#[cfg(not(target_os = "windows"))]
fn open_file_for_query(_path: &std::path::Path) -> DefragResult<isize> {
    // Stub: return a sentinel handle
    Ok(1)
}

#[cfg(target_os = "windows")]
fn close_file_handle(handle: isize) {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    unsafe {
        let _ = CloseHandle(HANDLE(handle as _));
    }
}

#[cfg(not(target_os = "windows"))]
fn close_file_handle(_handle: isize) {}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Tests
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_report_average_fragments_zero_files() {
        let rep = FragmentationReport::default();
        assert!((rep.average_fragments() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_report_fragmentation_percent() {
        let rep = FragmentationReport {
            total_files: 10,
            fragmented_files: 3,
            total_fragments: 14,
            ..Default::default()
        };
        assert!((rep.fragmentation_percent() - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_file_frag_info_total_clusters() {
        let info = FileFragInfo {
            path: PathBuf::from("test.txt"),
            fragment_count: 2,
            is_fragmented: true,
            runs: vec![
                ClusterRun { vcn: 0, lcn: 100, length: 10 },
                ClusterRun { vcn: 10, lcn: 500, length: 5 },
            ],
        };
        assert_eq!(info.total_clusters(), 15);
    }

    #[test]
    fn test_largest_first_sorting() {
        let mut rep = FragmentationReport {
            fragmented: vec![
                FileFragInfo {
                    path: PathBuf::from("small.bin"),
                    fragment_count: 50,
                    is_fragmented: true,
                    runs: vec![ClusterRun { vcn: 0, lcn: 1, length: 10 }],
                },
                FileFragInfo {
                    path: PathBuf::from("large.bin"),
                    fragment_count: 2,
                    is_fragmented: true,
                    runs: vec![ClusterRun { vcn: 0, lcn: 2, length: 100 }],
                },
            ],
            ..Default::default()
        };

        rep.fragmented.sort_unstable_by(|a, b| {
            b.total_clusters()
                .cmp(&a.total_clusters())
                .then_with(|| b.fragment_count.cmp(&a.fragment_count))
        });

        assert_eq!(rep.fragmented[0].path, PathBuf::from("large.bin"));
    }
}

