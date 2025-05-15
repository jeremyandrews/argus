pub mod entity_handling;
pub mod processing;
pub mod quality;
pub mod similarity;
pub mod worker_loop;

// Re-export the worker_loop module as the main interface
pub use worker_loop::*;
