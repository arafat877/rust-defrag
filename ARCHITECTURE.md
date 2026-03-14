# RustDefrag Architecture

RustDefrag is built using a modular layered architecture to ensure safety, maintainability, and extensibility.

## System Overview

```
┌─────────────────────────────────────────────────────────┐
│                      CLI Layer                          │
│                      (cli.rs)                           │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│                     Controller                          │
│                     (main.rs)                           │
└──────┬───────────────┬─────────────────┬────────────────┘
       │               │                 │
┌──────▼──────┐ ┌──────▼──────┐ ┌───────▼──────┐
│  Volume     │ │  Analyzer   │ │  Defrag      │
│  (volume.rs)│ │(analyzer.rs)│ │  (defrag.rs) │
└──────┬──────┘ └──────┬──────┘ └──────┬───────┘
       │               │               │
┌──────▼───────────────▼───────────────▼───────┐
│              Windows I/O Layer                │
│                 (winapi.rs)                   │
└───────────────────────┬───────────────────────┘
                        │
┌───────────────────────▼───────────────────────┐
│              NTFS Volume (disk)                │
└────────────────────────────────────────────────┘
```

## Module Responsibilities

### `cli.rs` — Argument Parser
- Parses Windows-style `/A /V /Q /H /?` flags using `clap`
- Normalises drive letters to Win32 device paths (`C:` → `\\.\C:`)
- Validates input before any I/O occurs

### `main.rs` — Controller
- Top-level workflow orchestration
- Privilege verification
- Phase sequencing: open → enumerate → analyse → defrag → report
- Terminal output formatting

### `volume.rs` — Volume Layer
- Opens the NTFS volume device handle
- Queries cluster geometry (`GetDiskFreeSpaceW`)
- Loads the volume cluster bitmap (`FSCTL_GET_VOLUME_BITMAP`)
- Enumerates files recursively with system/temp tagging

### `analyzer.rs` — Fragmentation Analyser
- Parallel file scan using `rayon`
- Retrieves cluster run lists via `FSCTL_GET_RETRIEVAL_POINTERS`
- Computes per-file fragment counts
- Builds the aggregate `FragmentationReport`

### `defrag.rs` — Defrag Engine
- Iterates fragmented files (worst-first)
- Locates contiguous free regions in the bitmap
- Calls `FSCTL_MOVE_FILE` for each cluster run
- Updates the in-memory bitmap after each move
- Enforces the protected-file list

### `winapi.rs` — Windows API Boundary
- **All `unsafe` code lives here and only here**
- Wraps `DeviceIoControl` with typed Rust structures
- Provides compile-time stubs for non-Windows builds
- Maps Win32 error codes to `DefragError` variants

### `progress.rs` — UI
- `ProgressReporter`: dual-bar layout (scan + defrag) via `indicatif`
- `Spinner`: indeterminate spinner for quick operations
- Quiet mode suppresses all progress output

### `errors.rs` — Error Types
- Typed `DefragError` enum with user-friendly messages
- `DefragResult<T>` alias for `Result<T, anyhow::Error>`
- `api_error()` helper for wrapping Win32 codes

## Data Flow

```
FileEntry[] ──► analyse_files() ──► FragmentationReport
                                         │
                                    fragmented[]
                                         │
                                    defragment() ──► DefragStats
                                         │
                                    VolumeBitmap (mutated in-place)
```

## Design Goals

- **Safety**: `unsafe` is quarantined to `winapi.rs`; all Win32 handles are RAII-wrapped
- **Modularity**: each module has a single clear responsibility and is independently testable
- **Testability**: non-Windows stubs let the full test suite run on Linux CI
- **Performance**: parallel scanning via `rayon`; sequential cluster moves (required by Windows)
- **Resilience**: any per-file failure is logged and skipped; the overall operation never aborts
