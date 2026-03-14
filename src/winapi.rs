/// Author : Arafat BOUCHAFRA <arafat877@gmail.com>
/// winapi.rs â€” Safe wrappers around Windows filesystem control APIs
///
/// All raw `unsafe` Win32 calls are isolated here so the rest of the
/// codebase remains safe Rust.  Each wrapper maps a Windows error code
/// to a typed [`DefragError`].
///
/// Compilation note: this file is compiled **only on Windows** via the
/// `#[cfg(target_os = "windows")]` attribute on the public API surface.
/// On non-Windows platforms the stubs let the project compile (and tests
/// run) on Linux/macOS CI runners.

use crate::errors::{DefragError, DefragResult};
use log::debug;

// â”€â”€ Platform guard â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(target_os = "windows")]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        Storage::FileSystem::{
            CreateFileW, GetVolumeInformationW, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ,
            FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{IO::DeviceIoControl, Threading::OpenProcessToken},
    },
};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  IOCTL constants (not exported by the `windows` crate at this version)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub const FSCTL_GET_VOLUME_BITMAP: u32 = 0x0009_006F;
pub const FSCTL_GET_RETRIEVAL_POINTERS: u32 = 0x0009_0073;
pub const FSCTL_MOVE_FILE: u32 = 0x0009_0074;

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Raw C-compatible structures
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Input to `FSCTL_GET_VOLUME_BITMAP`.
#[repr(C)]
pub struct StartingLcnInputBuffer {
    pub starting_lcn: i64,
}

/// Variable-length output header for `FSCTL_GET_VOLUME_BITMAP`.
#[repr(C)]
pub struct VolumeBitmapBuffer {
    pub starting_lcn: i64,
    pub bitmap_size: i64,
    /// Bitmap bytes follow in memory; we read them via a slice offset.
    pub buffer: [u8; 1],
}

/// Input to `FSCTL_GET_RETRIEVAL_POINTERS`.
#[repr(C)]
pub struct StartingVcnInputBuffer {
    pub starting_vcn: i64,
}

/// One entry in the retrieval pointer output.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RetrievalPointerExtent {
    pub next_vcn: i64,
    pub lcn: i64,
}

/// Header of `FSCTL_GET_RETRIEVAL_POINTERS` output (extents follow in memory).
#[repr(C)]
pub struct RetrievalPointersBuffer {
    pub extent_count: u32,
    pub _padding: u32,
    pub starting_vcn: i64,
    pub extents: [RetrievalPointerExtent; 1], // actually extent_count entries
}

/// Input to `FSCTL_MOVE_FILE`.
#[repr(C)]
pub struct MoveFileData {
    pub file_handle: isize, // HANDLE as isize for FFI safety
    pub starting_vcn: i64,
    pub starting_lcn: i64,
    pub cluster_count: u32,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Privilege check
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Returns `true` when the current process token has the Administrators SID.
#[cfg(target_os = "windows")]
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            windows::Win32::System::Threading::GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevated() -> bool {
    // On non-Windows we allow tests to run; always return true.
    true
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Volume handle
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// An owned Win32 HANDLE that closes itself on drop.
pub struct VolumeHandle(
    #[cfg(target_os = "windows")] HANDLE,
    #[cfg(not(target_os = "windows"))] i64,
);

impl VolumeHandle {
    /// Raw handle value (for `DeviceIoControl` etc.).
    #[cfg(target_os = "windows")]
    pub fn raw(&self) -> HANDLE {
        self.0
    }

    #[cfg(not(target_os = "windows"))]
    pub fn raw(&self) -> i64 {
        self.0
    }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Open a volume device for exclusive read/write access.
///
/// `path` must be a device path such as `\\.\C:`.
pub fn open_volume(path: &str) -> DefragResult<VolumeHandle> {
    debug!("Opening volume: {}", path);

    #[cfg(target_os = "windows")]
    {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_NO_BUFFERING,
                None,
            )
        }
        .map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure {
                api: "CreateFileW",
                code,
            }
        })?;

        if handle == INVALID_HANDLE_VALUE {
            anyhow::bail!(DefragError::InvalidVolume(path.to_string()));
        }

        Ok(VolumeHandle(handle))
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Stub for non-Windows builds
        log::warn!("open_volume: non-Windows stub â€” volume operations disabled");
        Ok(VolumeHandle(0))
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Filesystem type query
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Returns the filesystem name string (e.g. `"NTFS"`, `"FAT32"`).
pub fn get_filesystem_type(drive_label: &str) -> DefragResult<String> {
    #[cfg(target_os = "windows")]
    {
        // Root path must end with backslash: C:\
        let root = format!("{}\\", drive_label);
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        let mut fs_name = vec![0u16; 64];
        unsafe {
            GetVolumeInformationW(
                PCWSTR(wide.as_ptr()),
                None,
                None,
                None,
                None,
                Some(fs_name.as_mut_slice()),
            )
        }
        .map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure {
                api: "GetVolumeInformationW",
                code,
            }
        })?;

        let name = String::from_utf16_lossy(
            &fs_name[..fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len())],
        );
        Ok(name)
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Stub
        Ok("NTFS".to_string())
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Volume bitmap
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Fetch the volume cluster bitmap starting from cluster 0.
///
/// Returns `(starting_lcn, total_clusters, bitmap_bytes)`.
pub fn get_volume_bitmap(vol: &VolumeHandle) -> DefragResult<(i64, i64, Vec<u8>)> {
    #[cfg(target_os = "windows")]
    {
        // First call with a moderate buffer; ERROR_MORE_DATA is expected on large volumes.
        let mut output: Vec<u8> = vec![0u8; 1 << 20]; // 1 MiB
        let input = StartingLcnInputBuffer { starting_lcn: 0 };
        let mut bytes_returned: u32 = 0;

        let first = unsafe {
            DeviceIoControl(
                vol.raw(),
                FSCTL_GET_VOLUME_BITMAP,
                Some(&input as *const _ as *const _),
                std::mem::size_of::<StartingLcnInputBuffer>() as u32,
                Some(output.as_mut_ptr() as *mut _),
                output.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        const ERROR_MORE_DATA: u32 = 234;
        if first.is_err() {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            if code != ERROR_MORE_DATA {
                anyhow::bail!(DefragError::ApiFailure {
                    api: "FSCTL_GET_VOLUME_BITMAP",
                    code,
                });
            }
        }

        // Parse the header
        let hdr = unsafe { &*(output.as_ptr() as *const VolumeBitmapBuffer) };
        let starting_lcn = hdr.starting_lcn;
        let total_clusters = hdr.bitmap_size;

        // Bitmap bytes start 16 bytes into the output (two i64 fields)
        let bitmap_offset = 16usize;
        let bitmap_bytes = (total_clusters as usize + 7) / 8;

        // If the first output was truncated, issue a second call sized for the full bitmap.
        let required = bitmap_offset + bitmap_bytes;
        if output.len() < required {
            output.resize(required, 0);
            bytes_returned = 0;
            unsafe {
                DeviceIoControl(
                    vol.raw(),
                    FSCTL_GET_VOLUME_BITMAP,
                    Some(&input as *const _ as *const _),
                    std::mem::size_of::<StartingLcnInputBuffer>() as u32,
                    Some(output.as_mut_ptr() as *mut _),
                    output.len() as u32,
                    Some(&mut bytes_returned),
                    None,
                )
            }
            .map_err(|_| {
                let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
                DefragError::ApiFailure {
                    api: "FSCTL_GET_VOLUME_BITMAP",
                    code,
                }
            })?;
        }

        let bitmap = output[bitmap_offset..bitmap_offset + bitmap_bytes].to_vec();

        debug!(
            "Bitmap: starting_lcn={} total_clusters={} bitmap_bytes={}",
            starting_lcn, total_clusters, bitmap_bytes
        );

        Ok((starting_lcn, total_clusters, bitmap))
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Stub â€” return a synthetic 1 M cluster bitmap, half free
        let total_clusters: i64 = 1_000_000;
        let bitmap_bytes = (total_clusters as usize + 7) / 8;
        let mut bitmap = vec![0u8; bitmap_bytes];
        // Mark every other byte as used to simulate fragmentation
        for i in (0..bitmap_bytes).step_by(2) {
            bitmap[i] = 0xAA;
        }
        Ok((0, total_clusters, bitmap))
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Retrieval pointers
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// One contiguous cluster run for a file.
#[derive(Debug, Clone)]
pub struct ClusterRun {
    /// Virtual Cluster Number (offset within the file).
    pub vcn: i64,
    /// Logical Cluster Number (physical location on disk).
    pub lcn: i64,
    /// Number of clusters in this run.
    pub length: i64,
}

/// Retrieve the cluster runs for an open file handle.
pub fn get_retrieval_pointers(file_handle: isize) -> DefragResult<Vec<ClusterRun>> {
    #[cfg(target_os = "windows")]
    {
        let handle = HANDLE(file_handle as _);
        let mut runs = Vec::new();
        let mut starting_vcn: i64 = 0;
        let buf_size: usize = 1 << 16; // 64 KiB â€” enough for ~2700 extents
        let mut output: Vec<u8> = vec![0u8; buf_size];

        loop {
            let input = StartingVcnInputBuffer { starting_vcn };
            let mut bytes_returned: u32 = 0;

            let result = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_GET_RETRIEVAL_POINTERS,
                    Some(&input as *const _ as *const _),
                    std::mem::size_of::<StartingVcnInputBuffer>() as u32,
                    Some(output.as_mut_ptr() as *mut _),
                    buf_size as u32,
                    Some(&mut bytes_returned),
                    None,
                )
            };

            const ERROR_MORE_DATA: u32 = 234;
            let more_data = match &result {
                Err(_) => {
                    let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
                    if code == ERROR_MORE_DATA {
                        true
                    } else {
                        // EOF is expected when the file has no more extents
                        break;
                    }
                }
                Ok(_) => false,
            };

            let hdr = unsafe { &*(output.as_ptr() as *const RetrievalPointersBuffer) };
            let extent_count = hdr.extent_count as usize;
            let base_vcn = hdr.starting_vcn;

            let extents = unsafe {
                let ptr = &hdr.extents as *const RetrievalPointerExtent;
                std::slice::from_raw_parts(ptr, extent_count)
            };

            let mut vcn = base_vcn;
            for ext in extents {
                let length = ext.next_vcn - vcn;
                if ext.lcn != -1 {
                    // lcn == -1 means sparse/compressed; skip
                    runs.push(ClusterRun {
                        vcn,
                        lcn: ext.lcn,
                        length,
                    });
                }
                vcn = ext.next_vcn;
            }

            if !more_data {
                break;
            }
            if let Some(last) = extents.last() {
                starting_vcn = last.next_vcn;
            } else {
                break;
            }
        }

        Ok(runs)
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Stub â€” synthetic fragmented file
        Ok(vec![
            ClusterRun { vcn: 0, lcn: 100, length: 10 },
            ClusterRun { vcn: 10, lcn: 500, length: 8 },
        ])
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Move file
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Call `FSCTL_MOVE_FILE` to relocate `cluster_count` clusters.
pub fn move_file_clusters(
    vol: &VolumeHandle,
    file_handle: isize,
    starting_vcn: i64,
    starting_lcn: i64,
    cluster_count: u32,
) -> DefragResult<()> {
    #[cfg(target_os = "windows")]
    {
        let input = MoveFileData {
            file_handle,
            starting_vcn,
            starting_lcn,
            cluster_count,
        };

        let mut bytes_returned: u32 = 0;

        unsafe {
            DeviceIoControl(
                vol.raw(),
                FSCTL_MOVE_FILE,
                Some(&input as *const _ as *const _),
                std::mem::size_of::<MoveFileData>() as u32,
                None,
                0,
                Some(&mut bytes_returned),
                None,
            )
        }
        .map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure {
                api: "FSCTL_MOVE_FILE",
                code,
            }
        })?;

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        debug!(
            "move_file_clusters stub: fh={} vcn={} lcn={} count={}",
            file_handle, starting_vcn, starting_lcn, cluster_count
        );
        Ok(())
    }
}

