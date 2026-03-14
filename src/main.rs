/// main.rs — RustDefrag entry point and top-level controller
///
/// Coordinates the full workflow:
///   CLI parse → privilege check → volume open → filesystem check →
///   scan → report → optional defrag → summary

mod analyzer;
mod cli;
mod defrag;
mod errors;
mod progress;
mod volume;
mod winapi;

use anyhow::Context;
use colored::Colorize;
use log::info;
use std::time::{Duration, Instant};

use crate::cli::CliArgs;
use crate::errors::DefragError;
use crate::progress::{ProgressReporter, Spinner};

fn main() {
    // Initialise the logger — RUST_LOG env var controls verbosity
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    if let Err(e) = run() {
        eprintln!("{} {:#}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    // ── 1. Parse CLI ──────────────────────────────────────────────────────────
    let args = CliArgs::parse().context("Failed to parse arguments")?;

    if !args.quiet {
        print_banner();
        println!(
            "  {} Close running applications for best defragmentation results. Files in use by active processes will be skipped.\n",
            "⚠".yellow().bold()
        );
    }

    // ── 2. Privilege check ────────────────────────────────────────────────────
    if !winapi::is_elevated() {
        anyhow::bail!(DefragError::InsufficientPrivileges);
    }

    // ── 3. Open volume ────────────────────────────────────────────────────────
    let spin = Spinner::new(&format!("Opening volume {} …", args.drive_label));
    let (vol_handle, vol_info) =
        volume::open_volume(&args.drive, &args.drive_label).with_context(|| {
            format!(
                "Cannot open volume {} (device {}). Ensure this terminal is running as Administrator and the volume is not exclusively locked.",
                args.drive_label, args.drive
            )
        })?;
    spin.finish_ok(&format!(
        "Volume {} opened  [{} · cluster {}]",
        vol_info.label,
        vol_info.filesystem,
        format_bytes(vol_info.cluster_size)
    ));

    if !args.quiet {
        print_volume_summary(&vol_info);
    }

    // ── 4. Filesystem check ───────────────────────────────────────────────────
    if !vol_info.filesystem.eq_ignore_ascii_case("NTFS") {
        anyhow::bail!(DefragError::UnsupportedFilesystem(
            vol_info.filesystem.clone()
        ));
    }

    // ── 5. Set process priority (optional) ───────────────────────────────────
    if args.high_priority {
        set_high_priority();
    }

    // ── 6. Enumerate files ────────────────────────────────────────────────────
    let root = std::path::PathBuf::from(format!("{}\\", args.drive_label));
    let spin2 = Spinner::new("Enumerating files …");
    let mut last_ui_tick = Instant::now();
    let files = volume::enumerate_files_with_progress(&root, |count| {
        if last_ui_tick.elapsed() >= Duration::from_millis(250) {
            spin2.set_message(&format!("Enumerating files … {} found", count));
            last_ui_tick = Instant::now();
        }
    })
    .context("File enumeration failed")?;
    spin2.finish_ok(&format!("Found {} files", files.len()));

    // ── 7. Analyse fragmentation ──────────────────────────────────────────────
    let reporter = ProgressReporter::new(args.quiet);
    reporter.set_scan_total(files.len());

    if !args.quiet {
        println!(
            "\n{}",
            format!(" Analysing {} …", args.drive_label)
                .cyan()
                .bold()
        );
    }

    let report = analyzer::analyse_files(&files, args.verbose, |done, total| {
        reporter.set_scan_pos(done);
        if done == total {
            reporter.finish_scan();
        }
    })
    .context("Analysis failed")?;

    // ── 8. Print report ───────────────────────────────────────────────────────
    if !args.quiet {
        print_analysis_report(&report);
    }

    if args.analyze_only {
        if !args.quiet {
            println!(
                "\n{} Analysis only — skipping defragmentation (/A flag).",
                "ℹ".blue()
            );
        }
        return Ok(());
    }

    // ── 9. Defragmentation ────────────────────────────────────────────────────
    if report.fragmented.is_empty() {
        if !args.quiet {
            println!(
                "\n{} No fragmented files found. Volume is already optimised.",
                "✓".green()
            );
        }
        return Ok(());
    }

    if !args.quiet {
        println!(
            "\n{}",
            format!(
                " Defragmenting {} files …",
                report.fragmented_files
            )
            .green()
            .bold()
        );
    }

    let mut bitmap = volume::load_bitmap(&vol_handle).context("Cannot load volume bitmap")?;

    let stats = defrag::defragment(
        &vol_handle,
        &report.fragmented,
        &mut bitmap,
        &reporter,
        args.verbose,
    )
    .context("Defragmentation failed")?;

    // ── 10. Final summary ─────────────────────────────────────────────────────
    if !args.quiet {
        print_defrag_summary(&stats, &vol_info);
    }

    info!("RustDefrag completed successfully.");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Display helpers
// ─────────────────────────────────────────────────────────────────────────────

fn print_banner() {
    println!(
        r#"
  ____           _   ____         __
 |  _ \ _   _ __| |_|  _ \  ___ / _|_ __ __ _  __ _
 | |_) | | | / _` __| | | |/ _ \ |_| '__/ _` |/ _` |
 |  _ <| |_| \__ \ |_| |_| |  __/  _| | | (_| | (_| |
 |_| \_\\__,_|___/\__|____/ \___|_| |_|  \__,_|\__, |
                                                |___/
"#
    );
    println!(
        "  {} v{}\n",
        "RustDefrag".cyan().bold(),
        env!("CARGO_PKG_VERSION")
    );
}

fn print_volume_summary(info: &volume::VolumeInfo) {
    let used = info.total_clusters - info.free_clusters;
    let pct_used = if info.total_clusters > 0 {
        used as f64 / info.total_clusters as f64 * 100.0
    } else {
        0.0
    };

    println!("  Volume       : {}", info.label.yellow());
    println!("  Filesystem   : {}", info.filesystem);
    println!("  Cluster size : {}", format_bytes(info.cluster_size));
    println!(
        "  Total space  : {}",
        format_bytes(info.total_clusters as u64 * info.cluster_size)
    );
    println!(
        "  Free space   : {} ({:.1}% used)",
        format_bytes(info.free_clusters as u64 * info.cluster_size),
        pct_used
    );
    println!();
}

fn print_analysis_report(report: &analyzer::FragmentationReport) {
    println!("\n  {}", "── Fragmentation Report ──────────────────────".dimmed());
    println!("  Total files       : {}", report.total_files);
    println!(
        "  Fragmented files  : {}  ({:.1}%)",
        report.fragmented_files.to_string().yellow(),
        report.fragmentation_percent()
    );
    println!(
        "  Total fragments   : {}",
        report.total_fragments
    );
    println!(
        "  Average frags/file: {:.2}",
        report.average_fragments()
    );

    if let Some(worst) = &report.worst_file {
        println!(
            "  Most fragmented   : {:?}  ({} fragments)",
            worst.path.file_name().unwrap_or_default(),
            worst.fragment_count.to_string().red()
        );
    }
    println!();
}

fn print_defrag_summary(stats: &defrag::DefragStats, vol: &volume::VolumeInfo) {
    println!("\n  {}", "── Defragmentation Summary ───────────────────".dimmed());
    println!("  Files attempted   : {}", stats.files_attempted);
    println!(
        "  Files defragged   : {}",
        stats.files_defragged.to_string().green()
    );
    println!("  Files skipped     : {}", stats.files_skipped);
    println!("  Files in use      : {}", stats.files_in_use);
    println!(
        "  Clusters moved    : {}  ({})",
        stats.clusters_moved,
        format_bytes(stats.clusters_moved * vol.cluster_size)
    );
    println!("\n  {} Defragmentation complete.\n", "✓".green().bold());
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Priority helper
// ─────────────────────────────────────────────────────────────────────────────

fn set_high_priority() {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
        };
        unsafe {
            let _ = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
        }
    }
    info!("Process priority set to HIGH.");
}


