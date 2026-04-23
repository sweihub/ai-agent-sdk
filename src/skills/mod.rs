//! Skills module - ported from ~/claudecode/openclaudecode/src/skills
//!
//! This module provides the bundled skills infrastructure for the Rust SDK.

pub mod bundled;
pub mod bundled_skills;
pub mod loader;

pub use bundled::init_bundled_skills;
pub use bundled_skills::*;
pub use loader::{
    LoadedSkill, SkillMetadata, SkillSource, UnifiedSkill, load_all_skills, load_skill_from_dir,
    load_skills_from_dir, get_user_skills_dir, get_project_skills_dir,
};
