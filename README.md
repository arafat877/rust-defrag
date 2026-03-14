# RustDefrag

**A minimal NTFS defragmentation utility implemented in Rust - compatible with Windows `defrag.exe` CLI conventions.**

[![CI](https://github.com/arafat877/rust-defrag/actions/workflows/ci.yml/badge.svg)](https://github.com/arafat877/rust-defrag/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org/)

---

## Overview

RustDefrag is a systems-level tool that interacts directly with NTFS via `DeviceIoControl`, performing the same core operations as Windows `defrag.exe`:

1. **Analyze** - scan the volume, retrieve cluster run lists per file, and compute fragmentation metrics.
2. **Defragment** - move fragmented cluster runs into contiguous free regions with `FSCTL_MOVE_FILE`.

The project is written in **safe Rust**, with a small isolated `unsafe` boundary in `winapi.rs`.

---

## Features

| Feature | Status |
|---|---|
| NTFS volume analysis | OK (MVP) |
| Fragmentation statistics | OK (MVP) |
| Cluster relocation (`FSCTL_MOVE_FILE`) | OK (MVP) |
| Progress display (indicatif) | OK (MVP) |
| Administrator privilege detection | OK (MVP) |
| Safe file skip (pagefile, hiberfil, `$MFT`, etc.) | OK (MVP) |
| Parallel file scanning (rayon) | OK (MVP) |
| FAT32 / exFAT support | TODO (Phase 2) |
| Disk visualizer / heatmap | TODO (Phase 3) |
| Scheduler / enterprise policies | TODO (Phase 4) |

---

## Installation

### Prerequisites

- Windows 10 / 11 (x64)
- [Rust toolchain](https://www.rust-lang.org/tools/install) 1.75+
- MSVC build tools (`x86_64-pc-windows-msvc` target)

### Build from source

```powershell
git clone https://github.com/arafat877/rust-defrag
cd rust-defrag
cargo build --release
```

Binary path:

```text
target\release\defrag.exe
```

### Run

> **Administrator rights required.** Right-click PowerShell -> *Run as administrator*.

```powershell
.\defrag.exe C:         # Analyze + defragment
.\defrag.exe C: /A      # Analyze only
.\defrag.exe C: /V      # Verbose output
.\defrag.exe C: /Q      # Quiet (errors only)
.\defrag.exe C: /H      # High process priority
.\defrag.exe /?         # Help
```

---

## Usage

```text
USAGE:
    defrag <VOLUME> [OPTIONS]

ARGUMENTS:
    <VOLUME>    Drive letter, e.g. C:

OPTIONS:
    /A    Analyze only - print report, do not defragment
    /V    Verbose - per-file progress output
    /Q    Quiet - suppress all output except errors
    /H    High priority - elevate OS scheduling class
    /?    Show this help message
```

### Example session

```text
RustDefrag v0.1.0

Volume       : C:
Filesystem   : NTFS
Cluster size : 4 KB
Total space  : 476.84 GB
Free space   : 198.21 GB (58.4% used)

Analyzing C: ...
[####################---------] 72% (files analyzed)

-- Fragmentation Report --------------------------------
Total files       : 124,892
Fragmented files  : 1,247  (1.0%)
Total fragments   : 3,891
Average frags/file: 1.03
Most fragmented   : "build_output.bin"  (47 fragments)

Defragmenting 1,247 files ...
[#########################----] 91% (files processed)

-- Defragmentation Summary ------------------------------
Files attempted   : 1,247
Files defragged   : 1,198
Files skipped     : 49
Clusters moved    : 892,104  (3.39 GB)

OK: Defragmentation complete.
```

---

## Architecture

```text
CLI (cli.rs)
  |
  v
main.rs -> privilege check (winapi::is_elevated)
  |
  +-> volume.rs   -> open_volume / get_cluster_size / enumerate_files
  |     |
  |     +-> winapi.rs -> CreateFileW / DeviceIoControl
  |
  +-> analyzer.rs -> analyze_files (parallel via rayon)
  |     |
  |     +-> winapi::get_retrieval_pointers (FSCTL_GET_RETRIEVAL_POINTERS)
  |
  +-> defrag.rs   -> defragment / defrag_single_file
  |     |
  |     +-> winapi::move_file_clusters (FSCTL_MOVE_FILE)
  |
  +-> progress.rs -> ProgressReporter / Spinner (indicatif)
  +-> errors.rs   -> DefragError / DefragResult
```

### Module responsibilities

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point, workflow orchestration, output |
| `cli.rs` | Argument parsing, Windows `/FLAG` normalization |
| `volume.rs` | Volume open, bitmap load, file enumeration |
| `analyzer.rs` | Parallel fragmentation scan and report |
| `defrag.rs` | Cluster relocation and safety filters |
| `winapi.rs` | Isolated `unsafe` Win32 boundary |
| `progress.rs` | `indicatif` progress reporting |
| `errors.rs` | Typed error model and integration |

---

## Safety

RustDefrag does **not** move:

- NTFS metadata files (`$MFT`, `$LogFile`, `$Bitmap`, `$Volume`, etc.)
- `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`
- Files with `FILE_ATTRIBUTE_SYSTEM` or `FILE_ATTRIBUTE_TEMPORARY`

On move failure, the file is logged and skipped - the full operation continues.

---

## Development

```powershell
cargo fmt
cargo clippy
cargo test
cargo build
cargo build --release
```

### Running tests on non-Windows

`winapi.rs` uses `#[cfg(target_os = "windows")]` guards with non-Windows stubs so `cargo test` can run on Linux/macOS CI.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

Quick summary:
1. Fork the repository.
2. Create a branch (`feature/my-improvement`).
3. Run `cargo fmt && cargo clippy && cargo test`.
4. Open a pull request.

---

## Documentation

Full technical documentation: [`docs/RustDefragDocumentation.pdf`](docs/RustDefragDocumentation.pdf).

---

## Roadmap

### Phase 2
- SSD TRIM awareness
- Free-space consolidation pass

### Phase 3
- Disk cluster visualizer (ASCII heatmap)
- Fragmentation history tracking

### Phase 4
- Scheduled defrag via Windows Task Scheduler
- Enterprise disk policy engine

---

## License

MIT - see [LICENSE](LICENSE).

---

## Acknowledgements

- [Microsoft NTFS documentation](https://docs.microsoft.com/en-us/windows/win32/fileio/ntfs-technical-reference)
- [`windows`](https://crates.io/crates/windows) crate
- [`indicatif`](https://crates.io/crates/indicatif), [`clap`](https://crates.io/crates/clap), [`rayon`](https://crates.io/crates/rayon)
