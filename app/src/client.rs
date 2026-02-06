//! HTTP client for moltbook-server API.

use anyhow::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

/// API error types.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Agent name already taken: {0}")]
    NameTaken(String),
    
    #[error("API error: {0}")]
    Other(String),
    
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// API client for moltbook-server.
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
    auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthMeResponse {
    pub user_id: String,
    pub has_wallet: bool,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FundResponse {
    pub funded: bool,
    pub tx_hash: String,
    pub amount: String,
}

#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    pub balance: String,
    pub balance_formatted: String,
}

#[derive(Debug, Deserialize)]
pub struct StoreAgentResponse {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MoltbookStatusResponse {
    pub status: String,
    pub claimed: bool,
}

#[derive(Debug, Deserialize)]
pub struct CompileResponse {
    pub success: bool,
    pub compiled_hex: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    pub block_hash: String,
    pub block_number: u32,
    pub events: Vec<ChainEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainEvent {
    pub pallet: String,
    pub variant: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentInfo {
    pub chain_info: Option<ChainAgentInfo>,
    pub moltbook_info: Option<MoltbookAgentInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainAgentInfo {
    pub owner: String,
    pub name: String,
    pub active: bool,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoltbookAgentInfo {
    pub name: String,
    pub description: Option<String>,
    pub claimed: bool,
    pub twitter_handle: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoltbookPost {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub submolt: Option<SubmoltInfo>,
    pub created_at: String,
    #[serde(default)]
    pub upvotes: u32,
    #[serde(default)]
    pub downvotes: u32,
    #[serde(default)]
    pub comment_count: u32,
    #[serde(default)]
    pub author: Option<AuthorInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmoltInfo {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PostsResponse {
    pub posts: Vec<MoltbookPost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListItem {
    pub id: String,
    pub name: String,
    pub chain_address: Option<String>,
    pub created_at: String,
}

// ============================================================================
// Chain Event Types (decoded from server)
// ============================================================================

/// Decoded chain event received via SSE.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChainEventData {
    /// Agent run started
    RunStarted {
        run_id: u64,
        agent_name: String,
        caller: String,
    },
    /// Messages from the conversation
    Messages {
        run_id: u64,
        messages: Vec<ChatMessage>,
    },
    /// Tool execution started
    ToolsStarted {
        run_id: u64,
        tools: Vec<String>,
    },
    /// Tool execution completed
    ToolsCompleted {
        run_id: u64,
        tools: Vec<String>,
    },
    /// Agent is waiting for user input
    WaitingForInput {
        run_id: u64,
        reason: String,
        #[serde(default)]
        timeout_block: Option<u64>,
    },
    /// Agent run resumed
    Resumed {
        run_id: u64,
    },
    /// Agent run completed
    Completed {
        run_id: u64,
        output: String,
    },
    /// Agent run failed
    Failed {
        run_id: u64,
        reason: String,
    },
    /// Routing decision
    Routing {
        run_id: u64,
        result: bool,
        next_node: Option<u32>,
    },
    /// Raw/unknown event
    Raw {
        variant: String,
        data: String,
    },
}

/// A message in the agent conversation.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    /// System prompt
    System { content: String },
    /// User input
    User { content: String },
    /// Assistant response
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCallInfo>,
        #[serde(default)]
        output: Option<String>,
    },
    /// Tool execution result
    ToolResult {
        tool_name: String,
        call_id: u64,
        success: bool,
        result: String,
    },
}

/// Information about a tool call.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallInfo {
    pub call_id: u64,
    pub name: String,
    pub arguments: String,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
            auth_token: None,
        }
    }

    pub fn set_auth_token(&mut self, token: String) {
        self.auth_token = Some(token);
    }

    pub fn clear_auth_token(&mut self) {
        self.auth_token = None;
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(&url);

        if let Some(token) = &self.auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let error = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error: {}", error);
        }

        Ok(resp.json().await?)
    }

    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).json(body);

        if let Some(token) = &self.auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let error = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error: {}", error);
        }

        Ok(resp.json().await?)
    }

    /// Get OAuth URL for login.
    pub async fn get_auth_url(&self, redirect_port: u16) -> Result<String> {
        self.get(&format!("/auth/url?redirect_port={}", redirect_port))
            .await
    }

    /// Get current user info.
    pub async fn get_me(&self) -> Result<AuthMeResponse> {
        self.get("/auth/me").await
    }

    /// Get wallet balance (public endpoint, no auth required).
    pub async fn get_balance(&self, address: &str) -> Result<BalanceResponse> {
        let url = format!("{}/chain/balance?address={}", self.base_url, urlencoding::encode(address));
        let resp = self.http.get(&url).send().await?;
        
        if !resp.status().is_success() {
            let error = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error: {}", error);
        }
        
        Ok(resp.json().await?)
    }

    /// Fund wallet.
    pub async fn fund_wallet(&self, public_key: &str) -> Result<FundResponse> {
        self.post("/auth/fund", &serde_json::json!({ "public_key": public_key }))
            .await
    }

    /// Store an agent after TUI has registered with Moltbook directly.
    pub async fn store_agent(
        &self,
        name: &str,
        moltbook_api_key: &str,
    ) -> Result<StoreAgentResponse> {
        self.post(
            "/agents/store",
            &serde_json::json!({
                "name": name,
                "moltbook_api_key": moltbook_api_key
            }),
        )
        .await
    }

    /// Update an agent's chain address after successful deployment.
    pub async fn update_agent_address(
        &self,
        agent_id: &str,
        chain_address: &str,
    ) -> Result<()> {
        let url = format!("{}/agents/update-address", self.base_url);
        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                format!("Bearer {}", self.auth_token.as_deref().unwrap_or("")),
            )
            .json(&serde_json::json!({
                "agent_id": agent_id,
                "chain_address": chain_address
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to update agent address: {}", error);
        }

        Ok(())
    }

    /// Get Moltbook claim status using the API key directly.
    pub async fn get_moltbook_status(&self, api_key: &str) -> Result<MoltbookStatusResponse> {
        self.post(
            "/agents/moltbook-status",
            &serde_json::json!({
                "api_key": api_key
            }),
        )
        .await
    }

    /// Compile agent.
    pub async fn compile(
        &self,
        agent_id: &str,
        ship_file: &str,
        soul_md: &str,
        skill_md: &str,
        heartbeat_md: &str,
        schedule_blocks: Option<u32>,
    ) -> Result<CompileResponse> {
        let url = format!("{}/agents/compile", self.base_url);

        let mut form = reqwest::multipart::Form::new()
            .text("agent_id", agent_id.to_string())
            .text("ship_file", ship_file.to_string())
            .text("soul_md", soul_md.to_string())
            .text("skill_md", skill_md.to_string())
            .text("heartbeat_md", heartbeat_md.to_string());

        if let Some(blocks) = schedule_blocks {
            form = form.text("schedule_blocks", blocks.to_string());
        }

        let mut req = self.http.post(&url).multipart(form);

        if let Some(token) = &self.auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let error = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error: {}", error);
        }

        Ok(resp.json().await?)
    }

    /// Submit signed extrinsic.
    pub async fn submit_extrinsic(&self, extrinsic_hex: &str) -> Result<SubmitResponse> {
        self.post(
            "/chain/submit",
            &serde_json::json!({ "extrinsic_hex": extrinsic_hex }),
        )
        .await
    }

    /// Get agent info.
    pub async fn get_agent(&self, address: &str) -> Result<AgentInfo> {
        self.get(&format!("/agents/{}", address)).await
    }

    /// Get agent posts.
    pub async fn get_posts(&self, address: &str) -> Result<PostsResponse> {
        self.get(&format!("/agents/{}/posts", address)).await
    }

    /// List user's agents.
    pub async fn list_agents(&self) -> Result<Vec<AgentListItem>> {
        self.get("/agents").await
    }

    /// Build deploy extrinsic data (server builds call data, TUI signs).
    pub async fn build_deploy(
        &self,
        compiled_hex: &str,
        salt_hex: &str,
        signer_address: &str,
        value: u128,
    ) -> Result<BuildExtrinsicResponse> {
        self.post(
            "/chain/build-deploy",
            &serde_json::json!({
                "compiled_hex": compiled_hex,
                "salt_hex": salt_hex,
                "signer_address": signer_address,
                "value": value,
            }),
        )
        .await
    }

    /// Build call_agent extrinsic data.
    pub async fn build_call(
        &self,
        agent_address: &str,
        input: &str,
        signer_address: &str,
    ) -> Result<BuildExtrinsicResponse> {
        self.post(
            "/chain/build-call",
            &serde_json::json!({
                "agent_address": agent_address,
                "input": input,
                "signer_address": signer_address,
            }),
        )
        .await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildExtrinsicResponse {
    pub call_data_hex: String,
    pub nonce: u64,
    pub genesis_hash: String,
    pub spec_version: u32,
    pub transaction_version: u32,
}
