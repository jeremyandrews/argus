pub mod aliases;
pub mod extraction;
pub mod matching;
pub mod normalizer;
pub mod repository;
pub mod types;

pub use aliases::*;
pub use extraction::*;
pub use matching::*;
pub use normalizer::*;
pub use repository::*;
pub use types::*;

// Re-exports from crate root
pub use crate::LLMParams;
pub use crate::WorkerDetail;

// Module-level constants
pub const TARGET_ENTITY: &str = "entity";
