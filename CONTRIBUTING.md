# Contributing to RustDefrag

Thank you for your interest in contributing to RustDefrag — a systems-level NTFS defragmentation utility written in Rust.

## Development Setup

1. Install Rust: https://www.rust-lang.org/tools/install
2. Clone the repository: `git clone https://github.com/yourname/rust-defrag`
3. Enter directory: `cd rust-defrag`
4. Build: `cargo build`
5. Run tests: `cargo test`

## Code Style

RustDefrag follows standard Rust conventions enforced by `rustfmt`.

Before committing, always run:

```powershell
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Branching Model

- `main` — stable, always passes CI
- `feature/...` — new feature branches
- `bugfix/...` — bug fix branches

## Pull Request Guidelines

Pull requests must:
- Include a clear description of the change
- Pass all CI checks (fmt, clippy, test, build)
- Add or update tests for new behaviour
- Not touch `pagefile.sys`, `hiberfil.sys` or NTFS metadata files in logic changes

## Areas for Contribution

- NTFS parsing improvements
- Cluster relocation optimisation
- Disk visualisation engine
- Documentation and examples
- Test infrastructure (mock volumes)

## Reporting Issues

When filing a bug include:
- Windows version and edition
- Filesystem type of the target volume
- The exact command used
- Full error output (run with `RUST_LOG=debug` for verbose logs)

## Code of Conduct

Be respectful and constructive. This is a technical project; contributions are evaluated on correctness and safety, not seniority.
