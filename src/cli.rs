/// cli.rs — Command-line interface for RustDefrag
///
/// Parses Windows-compatible flags  (/A  /V  /Q  /H  /?)
/// using `clap` under the hood while keeping the original UX.

use clap::Parser;

// ── Raw clap definition ────────────────────────────────────────────────────────

/// RustDefrag — Minimal NTFS Defragmentation Utility
///
/// Examples:
///   defrag C:
///   defrag C: /A
///   defrag C: /V
///   defrag D: /A /V
#[derive(Parser, Debug)]
#[command(
    name = "defrag",
    version = env!("CARGO_PKG_VERSION"),
    author,
    about = "RustDefrag — Minimal NTFS Defragmentation Utility (MVP)",
    long_about = None,
    disable_help_flag = true,    // we handle /? ourselves
    disable_version_flag = true, // reserve -V for verbose mode
)]
pub struct RawArgs {
    /// Target drive letter (e.g. C:)
    #[arg(value_name = "VOLUME")]
    pub drive: String,

    /// Analyze only — display fragmentation report without defragmenting
    #[arg(long = "A", short = 'A', action = clap::ArgAction::SetTrue)]
    pub analyze_only: bool,

    /// Verbose — print detailed per-file progress
    #[arg(long = "V", short = 'V', action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Quiet — suppress all output except errors
    #[arg(long = "Q", short = 'Q', action = clap::ArgAction::SetTrue)]
    pub quiet: bool,

    /// High priority — run at elevated OS scheduling priority
    #[arg(long = "H", short = 'H', action = clap::ArgAction::SetTrue)]
    pub high_priority: bool,

    /// Show help information
    #[arg(long = "?", action = clap::ArgAction::Help, help = "Show this help message")]
    pub help_flag: Option<bool>,
}

// ── Validated, domain-level args ──────────────────────────────────────────────

/// Validated CLI arguments used throughout the application.
#[derive(Debug, Clone)]
pub struct CliArgs {
    /// Normalised drive root, e.g. `C:` → `\\.\C:`
    pub drive: String,

    /// Human-readable label, e.g. `C:`
    pub drive_label: String,

    /// If true, perform analysis only; skip the move phase.
    pub analyze_only: bool,

    /// If true, print per-file details.
    pub verbose: bool,

    /// If true, suppress informational output.
    pub quiet: bool,

    /// If true, ask the OS for higher scheduling priority.
    pub high_priority: bool,
}

impl CliArgs {
    /// Parse and validate the process arguments.
    ///
    /// Accepts Windows-style flags (/A, /V, /Q, /H) by rewriting them to
    /// POSIX long-option form before handing off to `clap`.
    pub fn parse() -> anyhow::Result<Self> {
        let raw_args = Self::normalise_windows_flags();
        let raw = RawArgs::parse_from(raw_args);
        Self::validate(raw)
    }

    /// Replace `/A` → `--A`, `/V` → `--V`, etc. so clap can parse them.
    fn normalise_windows_flags() -> Vec<String> {
        std::env::args()
            .map(|arg| {
                if arg.starts_with('/') && arg.len() == 2 {
                    let flag = &arg[1..];
                    match flag.to_uppercase().as_str() {
                        "A" | "V" | "Q" | "H" => format!("--{}", flag.to_uppercase()),
                        "?" => "--?".to_string(),
                        _ => arg,
                    }
                } else {
                    arg
                }
            })
            .collect()
    }

    fn validate(raw: RawArgs) -> anyhow::Result<Self> {
        // Normalise the drive letter
        let drive_label = raw.drive.trim_end_matches('\\').to_uppercase();

        // Must look like  X:
        if drive_label.len() != 2
            || !drive_label.chars().next().unwrap_or(' ').is_ascii_alphabetic()
            || !drive_label.ends_with(':')
        {
            anyhow::bail!(
                "Invalid volume '{}'. Provide a drive letter such as C:",
                raw.drive
            );
        }

        // Build the Win32 device path  \\.\C:
        let drive = format!("\\\\.\\{}", drive_label);

        Ok(Self {
            drive,
            drive_label,
            analyze_only: raw.analyze_only,
            verbose: raw.verbose,
            quiet: raw.quiet,
            high_priority: raw.high_priority,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(drive: &str, flags: &[&str]) -> RawArgs {
        let mut argv = vec!["defrag".to_string(), drive.to_string()];
        argv.extend(flags.iter().map(|s| s.to_string()));
        RawArgs::parse_from(argv)
    }

    #[test]
    fn test_analyze_flag() {
        let raw = make_args("C:", &["--A"]);
        assert!(raw.analyze_only);
        assert!(!raw.verbose);
    }

    #[test]
    fn test_verbose_flag() {
        let raw = make_args("D:", &["--V"]);
        assert!(raw.verbose);
    }

    #[test]
    fn test_quiet_flag() {
        let raw = make_args("E:", &["--Q"]);
        assert!(raw.quiet);
    }

    #[test]
    fn test_combined_flags() {
        let raw = make_args("C:", &["--A", "--V"]);
        assert!(raw.analyze_only);
        assert!(raw.verbose);
    }

    #[test]
    fn test_drive_validation_valid() {
        let raw = make_args("C:", &[]);
        let result = CliArgs::validate(raw);
        assert!(result.is_ok());
        let args = result.unwrap();
        assert_eq!(args.drive_label, "C:");
        assert_eq!(args.drive, "\\\\.\\C:");
    }

    #[test]
    fn test_drive_validation_invalid() {
        let raw = make_args("NotADrive", &[]);
        let result = CliArgs::validate(raw);
        assert!(result.is_err());
    }
}



