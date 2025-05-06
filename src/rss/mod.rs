//! RSS feed processing module for Argus.
//!
//! This module handles the fetching, parsing, and processing of RSS feeds.

mod client;
mod fetcher;
mod parser;
mod test;
mod types;
mod util;

// Re-export types for backward compatibility
pub use self::types::*;

// Re-export specific functions for lib.rs to use
pub use self::fetcher::process_rss_urls;
pub use self::fetcher::rss_loop;
pub use self::test::test_rss_feed;

// Re-export other modules
pub use self::client::*;
pub use self::parser::*;
pub use self::util::*;
