use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
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

    /// Create a new ralph project at the given path.
    ///
    /// Creates the directory structure and writes initial files:
    /// - PROMPT.md with provided content (skipped if empty - creates a Claude instance)
    /// - IMPLEMENTATION_PLAN.md with template
    /// - specs/ directory
    pub fn create(
        path: PathBuf,
        prompt_content: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create directory if it doesn't exist
        if !path.exists() {
            fs::create_dir_all(&path)?;
        } else if !path.is_dir() {
            return Err(format!("Path exists but is not a directory: {}", path.display()).into());
        }

        let prompt_path = path.join("PROMPT.md");

        // Only write PROMPT.md if content is provided (non-empty)
        // Empty prompt = Claude instance (no prompt file)
        if !prompt_content.trim().is_empty() {
            fs::write(&prompt_path, prompt_content)?;
        }

        // Create IMPLEMENTATION_PLAN.md with template
        let plan_path = path.join("IMPLEMENTATION_PLAN.md");
        let plan_content = "# Implementation Plan\n\n## Tasks\n\n- [ ] First task goes here\n";
        fs::write(&plan_path, plan_content)?;

        // Create specs/ directory
        let specs_dir = path.join("specs");
        fs::create_dir_all(&specs_dir)?;

        let instructions_path = path.join("PRIORITY_INSTRUCTIONS.md");

        Ok(Self {
            root: path,
            prompt_path,
            plan_path,
            specs_dir,
            instructions_path,
        })
    }

    /// Append an instruction for the next iteration.
    ///
    /// Creates PRIORITY_INSTRUCTIONS.md if it doesn't exist, then appends
    /// a timestamped instruction entry.
    pub fn append_instruction(
        &self,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.instructions_path)?;

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

        writeln!(file, "## [{timestamp}]")?;
        writeln!(file)?;
        writeln!(file, "{text}")?;
        writeln!(file)?;
        writeln!(file, "---")?;
        writeln!(file)?;

        Ok(())
    }

    /// Add a high-priority task to the plan.
    ///
    /// Reads IMPLEMENTATION_PLAN.md, finds the first `- [ ]` task,
    /// inserts the new task before it, and writes the file back.
    pub fn prepend_task(&self, task: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Read current content, or create default if file doesn't exist
        let content = if self.plan_path.exists() {
            fs::read_to_string(&self.plan_path)?
        } else {
            "# Implementation Plan\n\n## Tasks\n\n".to_string()
        };

        // Find the first uncompleted task (- [ ])
        let new_task_line = format!("- [ ] {}", task);

        let mut lines: Vec<&str> = content.lines().collect();
        let mut insert_index = None;

        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("- [ ]") {
                insert_index = Some(i);
                break;
            }
        }

        let new_content = match insert_index {
            Some(idx) => {
                // Insert new task before the first uncompleted task
                lines.insert(idx, &new_task_line);
                lines.join("\n") + "\n"
            }
            None => {
                // No uncompleted tasks found, append to end
                if content.ends_with('\n') {
                    format!("{}{}\n", content, new_task_line)
                } else {
                    format!("{}\n{}\n", content, new_task_line)
                }
            }
        };

        fs::write(&self.plan_path, new_content)?;
        Ok(())
    }
}
