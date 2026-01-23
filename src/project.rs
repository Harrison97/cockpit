use std::path::PathBuf;

/// Represents a ralph project directory structure.
///
/// A ralph project is a directory containing the files needed to run a ralph loop,
/// including PROMPT.md (required) and IMPLEMENTATION_PLAN.md (optional).
#[allow(dead_code)]
pub struct RalphProject {
    /// Root directory of the project
    pub root: PathBuf,

    /// Path to PROMPT.md
    pub prompt_path: PathBuf,

    /// Path to IMPLEMENTATION_PLAN.md
    pub plan_path: PathBuf,

    /// Path to specs/ directory
    pub specs_dir: PathBuf,

    /// Path to PRIORITY_INSTRUCTIONS.md
    pub instructions_path: PathBuf,
}
