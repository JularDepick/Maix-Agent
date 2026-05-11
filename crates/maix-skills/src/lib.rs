//! SKILL system — loadable skill packages (Phase 4).

pub mod loader_registry;
pub mod loaders;
pub mod manifest;
pub mod registry;
pub mod sandbox;
pub mod scheduler;

pub use loader_registry::LoaderRegistry;
pub use manifest::{SkillManifest, SkillRuntime};
pub use registry::SkillRegistry;
pub use sandbox::SkillSandbox;
pub use scheduler::SkillScheduler;
