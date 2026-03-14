# RustDefrag

**A minimal NTFS defragmentation utility implemented in Rust â€” fully compatible with Windows `defrag.exe` CLI conventions.**

[![CI](https://github.com/arafat877/rust-defrag/actions/workflows/ci.yml/badge.svg)](https://github.com/arafat877/rust-defrag/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org/)

---

## Overview

RustDefrag is a systems-level tool that interacts directly with the Windows NTFS filesystem via `DeviceIoControl`, performing the same operations as the built-in `defrag.exe`:

1. **Analyse** â€” scan the volume, retrieve cluster run lists for every file, compute fragmentation statistics.
2. **Defragment** â€” move fragmented cluster runs into contiguous free regions using `FSCTL_MOVE_FILE`.

It is written entirely in **safe Rust** (with a minimal `unsafe` boundary in `winapi.rs`) and is structured as a modular, testable codebase.

---

## Features

| Feature | Status |
|---|---|
| NTFS volume analysis | âœ… MVP |
| Fragmentation statistics | âœ… MVP |
| Cluster relocation (`FSCTL_MOVE_FILE`) | âœ… MVP |
| Progress display (indicatif) | âœ… MVP |
| Administrator privilege detection | âœ… MVP |
| Safe file skip (pagefile, hiberfil, $MFT â€¦) | âœ… MVP |
| Parallel file scanning (rayon) | âœ… MVP |
| FAT32 / exFAT support | ðŸ”œ Phase 2 |
| Disk visualiser / heatmap | ðŸ”œ Phase 3 |
| Scheduler / enterprise policies | ðŸ”œ Phase 4 |

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

The binary is produced at:

```
target\release\defrag.exe
```

### Run

> **Administrator rights required.** Right-click PowerShell â†’ *Run as administrator*.

```powershell
.\defrag.exe C:         # Analyse + defragment
.\defrag.exe C: /A      # Analyse only
.\defrag.exe C: /V      # Verbose output
.\defrag.exe C: /Q      # Quiet (errors only)
.\defrag.exe C: /H      # High process priority
.\defrag.exe /?         # Help
```

---

## Usage

```
USAGE:
    defrag <VOLUME> [OPTIONS]

ARGUMENTS:
    <VOLUME>    Drive letter, e.g. C:

OPTIONS:
    /A    Analyze only â€” print report, do not defragment
    /V    Verbose â€” per-file progress output
    /Q    Quiet â€” suppress all output except errors
    /H    High priority â€” elevate OS scheduling class
    /?    Show this help message
```

### Example session

```
  RustDefrag v0.1.0

  Volume       : C:
  Filesystem   : NTFS
  Cluster size : 4 KB
  Total space  : 476.84 GB
  Free space   : 198.21 GB (58.4% used)

 Analysing C: â€¦
â–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–‘â–‘â–‘â–‘â–‘â–‘â–‘â–‘â–‘ 72% (files analysed)

â”€â”€ Fragmentation Report â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  Total files       : 124,892
  Fragmented files  : 1,247  (1.0%)
  Total fragments   : 3,891
  Average frags/file: 1.03
  Most fragmented   : "build_output.bin"  (47 fragments)

 Defragmenting 1,247 files â€¦
â–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–ˆâ–‘â–‘â–‘ 91% (files processed)

â”€â”€ Defragmentation Summary â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  Files attempted   : 1,247
  Files defragged   : 1,198
  Files skipped     : 49
  Clusters moved    : 892,104  (3.39 GB)

  âœ“ Defragmentation complete.
```

---

## Architecture

```
CLI (cli.rs)
    â”‚
    â–¼
main.rs  â”€â”€â–º privilege check (winapi::is_elevated)
    â”‚
    â”œâ”€â”€â–º volume.rs   â”€â”€â–º open_volume / get_cluster_size / enumerate_files
    â”‚        â”‚
    â”‚        â””â”€â”€â–º winapi.rs  â”€â”€â–º CreateFileW / DeviceIoControl
    â”‚
    â”œâ”€â”€â–º analyzer.rs â”€â”€â–º analyse_files (parallel via rayon)
    â”‚        â”‚
    â”‚        â””â”€â”€â–º winapi::get_retrieval_pointers (FSCTL_GET_RETRIEVAL_POINTERS)
    â”‚
    â”œâ”€â”€â–º defrag.rs   â”€â”€â–º defragment / defrag_single_file
    â”‚        â”‚
    â”‚        â””â”€â”€â–º winapi::move_file_clusters (FSCTL_MOVE_FILE)
    â”‚
    â”œâ”€â”€â–º progress.rs â”€â”€â–º ProgressReporter / Spinner (indicatif)
    â””â”€â”€â–º errors.rs   â”€â”€â–º DefragError / DefragResult
```

### Module responsibilities

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point, top-level workflow, display |
| `cli.rs` | Argument parsing, Windows `/FLAG` normalisation |
| `volume.rs` | Volume open, bitmap, file enumeration |
| `analyzer.rs` | Parallel fragmentation scan, report |
| `defrag.rs` | Cluster relocation, safety filters |
| `winapi.rs` | `unsafe` Win32 boundary (isolated here only) |
| `progress.rs` | `indicatif` progress bars |
| `errors.rs` | Typed error enum, `anyhow` integration |

---

## Safety

RustDefrag **never** touches:

- NTFS metadata files (`$MFT`, `$LogFile`, `$Bitmap`, `$Volume`, â€¦)
- `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`
- Files with `FILE_ATTRIBUTE_SYSTEM` or `FILE_ATTRIBUTE_TEMPORARY`

On any move failure the file is logged and skipped â€” the operation never aborts.

---

## Development

```powershell
cargo fmt          # Format code
cargo clippy       # Lint
cargo test         # Run unit + integration tests
cargo build        # Debug build
cargo build --release  # Optimised release build
```

### Running tests on non-Windows

The `winapi.rs` module uses compile-time `#[cfg(target_os = "windows")]` guards. All Windows API calls have stubs for non-Windows builds so `cargo test` passes on Linux/macOS CI runners.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution guide.

Quick summary:
1. Fork the repository
2. Create a feature branch (`feature/my-improvement`)
3. Run `cargo fmt && cargo clippy && cargo test`
4. Submit a pull request

---

## Documentation

Full technical documentation is in [`docs/RustDefragDocumentation.pdf`](docs/RustDefragDocumentation.pdf).

Topics covered:
- NTFS filesystem internals
- Windows defrag API deep-dive
- Rust module design decisions
- Algorithm walkthrough
- Testing strategy

---

## Roadmap

### Phase 2
- SSD TRIM awareness
- Free-space consolidation pass

### Phase 3
- Disk cluster visualiser (ASCII heatmap)
- Fragmentation history tracking

### Phase 4
- Scheduled defrag via Windows Task Scheduler
- Enterprise disk policy engine

---

## License

MIT â€” see [LICENSE](LICENSE).

---

## Acknowledgements

RustDefrag builds upon:
- [Microsoft NTFS documentation](https://docs.microsoft.com/en-us/windows/win32/fileio/ntfs-technical-reference)
- The [`windows`](https://crates.io/crates/windows) Rust crate
- [`indicatif`](https://crates.io/crates/indicatif), [`clap`](https://crates.io/crates/clap), [`rayon`](https://crates.io/crates/rayon)

