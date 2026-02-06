//! Embedded agent assets and file source abstraction.
//!
//! Provides built-in default agent files that are embedded in the binary,
//! with the option to use a custom directory for advanced users.

use rust_embed::RustEmbed;
use std::path::Path;

/// Embedded default agent files from the agent/ directory.
#[derive(RustEmbed)]
#[folder = "../agent/"]
#[include = "*.ship"]
#[include = "*.md"]
#[exclude = "README.md"]
pub struct AgentAssets;

/// Source for agent files - either embedded defaults or a custom directory.
#[derive(Debug, Clone)]
pub enum AgentSource {
    /// Use embedded default files.
    Embedded,
    /// Use files from a custom directory path.
    Custom(String),
}

impl Default for AgentSource {
    fn default() -> Self {
        Self::Embedded
    }
}

/// Validation result for a file.
#[derive(Debug, Clone)]
pub enum FileStatus {
    /// File exists and is readable.
    Present,
    /// File is missing (for optional files).
    Missing,
    /// File is required but missing.
    RequiredMissing,
}

/// Validation result for an agent source.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub ship_file: FileStatus,
    pub soul_md: FileStatus,
    pub skill_md: FileStatus,
    pub heartbeat_md: FileStatus,
}

impl ValidationResult {
    /// Check if the source is valid (all required files present).
    pub fn is_valid(&self) -> bool {
        !matches!(self.ship_file, FileStatus::RequiredMissing)
    }
}

impl AgentSource {
    /// Read a file from this source.
    pub fn read_file(&self, name: &str) -> Option<String> {
        match self {
            AgentSource::Embedded => AgentAssets::get(name)
                .map(|f| String::from_utf8_lossy(&f.data).to_string()),
            AgentSource::Custom(dir) => {
                let path = Path::new(dir).join(name);
                std::fs::read_to_string(path).ok()
            }
        }
    }

    /// Check if a file exists in this source.
    pub fn file_exists(&self, name: &str) -> bool {
        match self {
            AgentSource::Embedded => AgentAssets::get(name).is_some(),
            AgentSource::Custom(dir) => {
                let path = Path::new(dir).join(name);
                path.exists() && path.is_file()
            }
        }
    }

    /// Validate the source - check all required and optional files.
    pub fn validate(&self) -> ValidationResult {
        let check_file = |name: &str, required: bool| -> FileStatus {
            if self.file_exists(name) {
                FileStatus::Present
            } else if required {
                FileStatus::RequiredMissing
            } else {
                FileStatus::Missing
            }
        };

        ValidationResult {
            ship_file: check_file("moltbook_agent.ship", true),
            soul_md: check_file("SOUL.md", false),
            skill_md: check_file("SKILL.md", false),
            heartbeat_md: check_file("HEARTBEAT.md", false),
        }
    }

}
