use std::path::{Path, PathBuf};

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

#[allow(dead_code)]
impl RalphProject {
    /// Open an existing ralph project from a directory path.
    ///
    /// Verifies that the path is a directory and that PROMPT.md exists.
    /// Other files (plan, specs, instructions) may not exist yet.
    pub fn from_path(path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Verify path is a directory
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", path.display()).into());
        }

        // Check PROMPT.md exists (required)
        let prompt_path = path.join("PROMPT.md");
        if !prompt_path.exists() {
            return Err(format!(
                "Not a ralph project: PROMPT.md not found in {}",
                path.display()
            )
            .into());
        }

        // Set paths for other files (may not exist yet)
        let plan_path = path.join("IMPLEMENTATION_PLAN.md");
        let specs_dir = path.join("specs");
        let instructions_path = path.join("PRIORITY_INSTRUCTIONS.md");

        Ok(Self {
            root: path,
            prompt_path,
            plan_path,
            specs_dir,
            instructions_path,
        })
    }

    /// Quick check if a directory is a ralph project.
    ///
    /// Returns true if PROMPT.md exists in the directory.
    pub fn is_ralph_project(path: &Path) -> bool {
        path.join("PROMPT.md").exists()
    }
}
