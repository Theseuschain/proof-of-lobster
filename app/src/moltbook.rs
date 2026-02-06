//! Direct Moltbook API client for TUI.
//!
//! This calls the Moltbook API directly from the user's machine to avoid
//! server-side rate limiting (Moltbook limits registration to 1 per host per day).

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MOLTBOOK_API_BASE: &str = "https://www.moltbook.com/api/v1";

/// Moltbook API error types.
#[derive(Debug, Error)]
pub enum MoltbookError {
    #[error("Agent name already taken: {0}")]
    NameTaken(String),

    #[error("Moltbook API error: {0}")]
    Api(String),

    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// Response from registering an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub api_key: String,
    pub claim_url: String,
    pub verification_code: String,
}

/// Internal response structure from Moltbook API.
#[derive(Debug, Clone, Deserialize)]
struct MoltbookRegisterResponse {
    agent: MoltbookAgentRegistration,
    #[allow(dead_code)]
    important: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MoltbookAgentRegistration {
    api_key: String,
    claim_url: String,
    verification_code: String,
}

/// Agent status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
}

/// Agent info response from /agents/me endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeResponse {
    pub name: String,
    pub description: String,
    pub is_claimed: bool,
}

/// Internal response structure from Moltbook /agents/me API.
#[derive(Debug, Clone, Deserialize)]
struct MoltbookAgentMeResponse {
    agent: MoltbookAgentInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct MoltbookAgentInfo {
    name: String,
    description: String,
    is_claimed: bool,
}

/// Register a new agent with Moltbook.
pub async fn register_agent(name: &str, description: &str) -> Result<RegisterResponse, MoltbookError> {
    let client = reqwest::Client::new();
    let url = format!("{}/agents/register", MOLTBOOK_API_BASE);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "name": name,
            "description": description
        }))
        .send()
        .await?;

    let status = response.status();

    if !status.is_success() {
        let error = response.text().await.unwrap_or_default();

        // Check for "name already taken" error (409 Conflict)
        if status == reqwest::StatusCode::CONFLICT {
            // Try to parse the hint from the response
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&error) {
                if let Some(hint) = json.get("hint").and_then(|h| h.as_str()) {
                    return Err(MoltbookError::NameTaken(hint.to_string()));
                }
            }
            return Err(MoltbookError::NameTaken(format!(
                "The name \"{}\" is already taken. Please choose a different name.",
                name
            )));
        }

        return Err(MoltbookError::Api(format!(
            "Failed to register agent ({}): {}",
            status, error
        )));
    }

    // Parse the response
    let body_text = response.text().await?;
    let moltbook_resp: MoltbookRegisterResponse = serde_json::from_str(&body_text)
        .map_err(|e| MoltbookError::Api(format!("Failed to parse response: {}. Body: {}", e, body_text)))?;

    Ok(RegisterResponse {
        api_key: moltbook_resp.agent.api_key,
        claim_url: moltbook_resp.agent.claim_url,
        verification_code: moltbook_resp.agent.verification_code,
    })
}

/// Check agent claim status with Moltbook.
pub async fn get_status(api_key: &str) -> Result<StatusResponse, MoltbookError> {
    let client = reqwest::Client::new();
    let url = format!("{}/agents/status", MOLTBOOK_API_BASE);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await.unwrap_or_default();
        return Err(MoltbookError::Api(format!("Failed to get status: {}", error)));
    }

    Ok(response.json().await?)
}

/// Get agent info using an existing API key.
pub async fn get_agent_info(api_key: &str) -> Result<AgentMeResponse, MoltbookError> {
    let client = reqwest::Client::new();
    let url = format!("{}/agents/me", MOLTBOOK_API_BASE);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await.unwrap_or_default();
        return Err(MoltbookError::Api(format!("Invalid API key or agent not found: {}", error)));
    }

    let body_text = response.text().await?;
    let resp: MoltbookAgentMeResponse = serde_json::from_str(&body_text)
        .map_err(|e| MoltbookError::Api(format!("Failed to parse response: {}. Body: {}", e, body_text)))?;

    Ok(AgentMeResponse {
        name: resp.agent.name,
        description: resp.agent.description,
        is_claimed: resp.agent.is_claimed,
    })
}
