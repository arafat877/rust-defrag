# RustDefrag Architecture

RustDefrag uses a modular layered architecture focused on safety, maintainability, and predictable behavior on NTFS volumes.

## System Overview

```text
+-----------------------------+
|          CLI Layer          |
|          (cli.rs)           |
+-----------------------------+
              |
+-----------------------------+
|         Controller          |
|          (main.rs)          |
+-----------------------------+
   |             |            |
   v             v            v
+-----------+ +-----------+ +-----------+
| volume.rs | |analyzer.rs| | defrag.rs |
+-----------+ +-----------+ +-----------+
      \            |            /
       \           |           /
        +---------------------+
        |   Windows I/O API   |
        |      winapi.rs      |
        +---------------------+
                  |
        +---------------------+
        |   NTFS Volume Disk  |
        +---------------------+
```

## Module Responsibilities

### `cli.rs` - Argument Parser
- Parses Windows-style `/A /V /Q /H /?` flags via `clap`
- Normalizes drive letters to Win32 device paths (`C:` -> `\\.\C:`)
- Validates input before I/O

### `main.rs` - Controller
- Orchestrates full workflow
- Verifies privileges
- Sequences phases: open -> enumerate -> analyze -> defrag -> report
- Formats terminal output

### `volume.rs` - Volume Layer
- Opens NTFS volume handle
- Reads cluster geometry (`GetDiskFreeSpaceW`)
- Reads volume bitmap (`FSCTL_GET_VOLUME_BITMAP`)
- Enumerates files recursively

### `analyzer.rs` - Fragmentation Analyzer
- Parallel scan using `rayon`
- Reads cluster runs (`FSCTL_GET_RETRIEVAL_POINTERS`)
- Computes per-file fragment counts
- Builds `FragmentationReport`

### `defrag.rs` - Defrag Engine
- Iterates fragmented files (largest-first)
- Finds contiguous free regions in bitmap
- Moves runs with `FSCTL_MOVE_FILE`
- Updates in-memory bitmap after moves
- Skips protected and in-use files

### `winapi.rs` - Windows API Boundary
- Contains all `unsafe` code (isolated)
- Wraps `DeviceIoControl` with typed Rust structs
- Maps Win32 errors to `DefragError`
- Provides non-Windows stubs for tests

### `progress.rs` - Terminal UI
- `ProgressReporter`: scan and defrag progress bars
- `Spinner`: short operation status feedback
- Quiet mode support

### `errors.rs` - Error Types
- Central `DefragError` enum
- `DefragResult<T>` alias
- Consistent API and user-facing error conversion

## Data Flow

```text
FileEntry[] -> analyze_files() -> FragmentationReport
                                  |
                                  v
                             fragmented[]
                                  |
                                  v
                            defragment() -> DefragStats
                                  |
                                  v
                    VolumeBitmap (mutated in place)
```

## Design Goals

- Safety: `unsafe` stays in `winapi.rs`; handles are RAII-managed.
- Modularity: each module has a clear single responsibility.
- Testability: non-Windows stubs allow CI testing on Linux.
- Performance: parallel scanning via `rayon`, controlled move operations.
- Resilience: per-file failures are logged and skipped without aborting run.

Repository: https://github.com/arafat877/rust-defrag
