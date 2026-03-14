/// lib.rs — Public library surface for integration tests
///
/// This file re-exports the internal modules so integration tests in
/// `tests/` can access them without duplication.

pub mod analyzer;
pub mod cli;
pub mod defrag;
pub mod errors;
pub mod progress;
pub mod volume;
pub mod winapi;
