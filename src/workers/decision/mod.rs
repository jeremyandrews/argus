pub mod extraction;
pub mod processing;
pub mod threat;
pub mod worker_loop;

// Re-export the worker_loop module as the main interface
pub use worker_loop::*;
