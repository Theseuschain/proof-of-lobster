//! Local configuration management for Proof of Lobster.

use crate::agent_assets::AgentSource;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration stored locally.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Server URL
    pub server_url: String,

    /// Auth token from Supabase
    pub auth_token: Option<String>,

    /// Deployed agent address
    pub agent_address: Option<String>,

    /// Agent name
    pub agent_name: Option<String>,

    /// Custom agent directory path. If None, use embedded defaults.
    #[serde(default)]
    pub custom_agent_dir: Option<String>,
}

impl AppConfig {
    /// Get the config file path.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("proof-of-lobster")
            .join("config.json")
    }

    /// Load config from disk.
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Check if user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.auth_token.is_some()
    }

    /// Check if user has a deployed agent.
    pub fn has_agent(&self) -> bool {
        self.agent_address.is_some()
    }

    /// Clear auth (logout). Also clears agent data since it belongs to the logged-in user.
    pub fn logout(&mut self) {
        self.auth_token = None;
        // Agent data is tied to the authenticated user, so clear it on logout
        self.agent_address = None;
        self.agent_name = None;
    }

    /// Get the agent source based on config.
    pub fn agent_source(&self) -> AgentSource {
        match &self.custom_agent_dir {
            Some(dir) => AgentSource::Custom(dir.clone()),
            None => AgentSource::Embedded,
        }
    }
}
