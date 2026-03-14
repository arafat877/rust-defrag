/// Author : Arafat BOUCHAFRA <arafat877@gmail.com>
/// progress.rs - Terminal progress reporting using `indicatif`
///
/// Provides a consistent UI for the scan and defrag phases.
/// Quiet mode suppresses all bars; verbose mode adds file-level logging.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::time::Duration;

fn scan_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.cyan} [{elapsed_precise}] Scanning  [{bar:45.cyan/blue}] {pos}/{len} files",
    )
    .unwrap()
    .progress_chars("#>-")
}

fn defrag_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] Defragging [{bar:45.green/blue}] {pos}/{len} files",
    )
    .unwrap()
    .progress_chars("#>-")
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.yellow} {msg}")
        .unwrap()
        .tick_strings(&["|", "/", "-", "\\"])
}

/// Manages the `indicatif` progress bars for both phases.
pub struct ProgressReporter {
    multi: MultiProgress,
    scan_bar: ProgressBar,
    defrag_bar: Mutex<Option<ProgressBar>>,
    quiet: bool,
}

impl ProgressReporter {
    /// Create a new reporter. If `quiet` is true, all bars are hidden.
    pub fn new(quiet: bool) -> Self {
        let multi = MultiProgress::new();

        let scan_bar = if quiet {
            ProgressBar::hidden()
        } else {
            let pb = multi.add(ProgressBar::new(0));
            pb.set_style(scan_style());
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        };

        Self {
            multi,
            scan_bar,
            defrag_bar: Mutex::new(None),
            quiet,
        }
    }

    /// Set the total file count for the scan bar.
    pub fn set_scan_total(&self, total: usize) {
        self.scan_bar.set_length(total as u64);
    }

    /// Advance the scan bar to `pos`.
    pub fn set_scan_pos(&self, pos: usize) {
        self.scan_bar.set_position(pos as u64);
    }

    /// Mark the scan phase as complete.
    pub fn finish_scan(&self) {
        self.scan_bar.finish_with_message("Scan complete");
    }

    /// Set the total for the defrag bar.
    pub fn set_defrag_total(&self, total: u64) {
        if self.quiet {
            return;
        }

        let mut guard = self.defrag_bar.lock().unwrap();
        if guard.is_none() {
            let pb = self.multi.add(ProgressBar::new(0));
            pb.set_style(defrag_style());
            pb.enable_steady_tick(Duration::from_millis(100));
            *guard = Some(pb);
        }

        if let Some(pb) = guard.as_ref() {
            pb.set_length(total);
        }
    }

    /// Advance the defrag bar to `pos`.
    pub fn set_defrag_pos(&self, pos: u64) {
        if let Some(pb) = self.defrag_bar.lock().unwrap().as_ref() {
            pb.set_position(pos);
        }
    }

    /// Mark the defrag phase as complete.
    pub fn finish_defrag(&self) {
        if let Some(pb) = self.defrag_bar.lock().unwrap().as_ref() {
            pb.finish_with_message("Defragmentation complete");
        }
    }
}

/// A lightweight spinner for operations without a known count.
pub struct Spinner(ProgressBar);

impl Spinner {
    pub fn new(msg: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(spinner_style());
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));
        Spinner(pb)
    }

    pub fn set_message(&self, msg: &str) {
        self.0.set_message(msg.to_string());
    }

    pub fn finish_ok(&self, msg: &str) {
        self.0.finish_with_message(format!("OK  {}", msg));
    }

    #[allow(dead_code)]
    pub fn finish_err(&self, msg: &str) {
        self.0.finish_with_message(format!("ERR {}", msg));
    }
}

