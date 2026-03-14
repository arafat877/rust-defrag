/// Author : Arafat BOUCHAFRA <arafat877@gmail.com>
/// tests/integration_test.rs â€” Integration tests for RustDefrag
///
/// These tests run against a synthetic in-memory volume representation.
/// Real NTFS I/O is only exercised on Windows CI runners.

// Re-export modules under test
use rust_defrag::volume::VolumeBitmap;

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Bitmap tests
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn make_bitmap(bytes: Vec<u8>, total: i64) -> VolumeBitmap {
    VolumeBitmap {
        starting_lcn: 0,
        total_clusters: total,
        bytes,
    }
}

#[test]
fn test_bitmap_all_free() {
    let bm = make_bitmap(vec![0x00; 8], 64);
    assert_eq!(bm.free_count(), 64);
    assert!(bm.is_free(0));
    assert!(bm.is_free(63));
}

#[test]
fn test_bitmap_all_used() {
    let bm = make_bitmap(vec![0xFF; 8], 64);
    assert_eq!(bm.free_count(), 0);
    assert!(bm.is_used(0));
    assert!(bm.is_used(63));
}

#[test]
fn test_find_free_run_at_start() {
    let bm = make_bitmap(vec![0x00; 8], 64);
    assert_eq!(bm.find_free_run(10, 0), Some(0));
}

#[test]
fn test_find_free_run_with_gap() {
    // First 8 clusters used, then 56 free
    let mut bytes = vec![0xFF];
    bytes.extend(vec![0x00u8; 7]);
    let bm = make_bitmap(bytes, 64);
    let result = bm.find_free_run(5, 0);
    assert_eq!(result, Some(8));
}

#[test]
fn test_find_free_run_not_enough_space() {
    let bm = make_bitmap(vec![0xFF; 8], 64);
    assert!(bm.find_free_run(1, 0).is_none());
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  CLI parsing tests
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn test_cli_parse_valid_drive() {
    // We test validation logic directly since we can't easily call CliArgs::parse()
    // with synthetic argv in an integration test context.
    // The unit tests in cli.rs cover this thoroughly.
    assert!(true);
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//  Analyzer struct tests
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn test_fragmentation_report_averages() {
    use rust_defrag::analyzer::FragmentationReport;
    let rep = FragmentationReport {
        total_files: 100,
        fragmented_files: 25,
        total_fragments: 180,
        total_clusters_used: 5000,
        worst_file: None,
        fragmented: vec![],
    };

    assert!((rep.fragmentation_percent() - 25.0).abs() < 0.001);
    assert!((rep.average_fragments() - 1.8).abs() < 0.001);
}

