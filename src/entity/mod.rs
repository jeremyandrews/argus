pub mod extraction;
pub mod matching;
pub mod repository;
pub mod types;

pub use types::*;

// Re-exports from crate root
pub use crate::LLMParams;
pub use crate::WorkerDetail;

// Module-level constants
pub const TARGET_ENTITY: &str = "entity";
