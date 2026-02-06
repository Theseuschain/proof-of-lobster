//! Create agent wizard screen.

use crate::{
    agent_assets::{AgentSource, FileStatus, ValidationResult},
    app::{App, AppMessage, ScreenAction},
    client::ApiClient,
    extrinsic,
    screens::Screen,
    wallet::WalletConfig,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum CreateStep {
    /// Select agent file source (embedded or custom directory)
    SelectAgentSource,
    /// Enter agent name and description together
    EnterAgentInfo,
    /// Registering with Moltbook
    RegisteringMoltbook,
    /// Waiting for claim verification
    WaitingClaim,
    /// Review SOUL.md
    ReviewSoul,
    /// Configure schedule
    ConfigureSchedule,
    /// Compiling
    Compiling,
    /// Deploying
    Deploying,
    /// Success
    Success,
}

/// Which field is currently active in the agent info form
#[derive(Debug, Clone, PartialEq)]
pub enum AgentInfoField {
    Name,
    Description,
    ApiKey,
}

/// 1 UNIT = 1_000_000_000_000 planck (12 decimals)
const UNIT_PLANCK: u128 = 1_000_000_000_000;

/// Which field is active in the schedule/balance form
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleField {
    Schedule,
    CustomMinutes,
    Balance,
}

pub struct CreateScreen {
    pub step: CreateStep,
    // Agent source selection
    pub use_embedded: bool,
    pub custom_dir_input: String,
    pub source_validation: Option<ValidationResult>,
    // Agent info
    pub agent_name: String,
    pub agent_description: String,
    pub api_key_input: String,
    pub active_field: AgentInfoField,
    pub name_error: Option<String>,
    pub api_key_error: Option<String>,
    pub api_key_status: Option<String>,
    pub agent_id: Option<String>,
    pub moltbook_api_key: Option<String>,
    pub claim_url: Option<String>,
    pub verification_code: Option<String>,
    pub schedule_option: Option<u32>,
    pub compiled_hex: Option<String>,
    pub agent_address: Option<String>,
    pub error: Option<String>,
    pub selected_schedule: usize,
    pub custom_minutes_input: String,
    pub balance_input: String,
    pub balance_error: Option<String>,
    pub schedule_field: ScheduleField,
    pub value_planck: u128,
}

impl CreateScreen {
    pub fn new() -> Self {
        Self {
            step: CreateStep::SelectAgentSource,
            // Agent source - default to embedded
            use_embedded: true,
            custom_dir_input: String::new(),
            source_validation: None,
            // Agent info
            agent_name: String::new(),
            agent_description: String::new(),
            api_key_input: String::new(),
            active_field: AgentInfoField::Name,
            name_error: None,
            api_key_error: None,
            api_key_status: None,
            agent_id: None,
            moltbook_api_key: None,
            claim_url: None,
            verification_code: None,
            schedule_option: Some(600), // Default: 1 hour (600 blocks)
            compiled_hex: None,
            agent_address: None,
            error: None,
            selected_schedule: 2, // Index 2 = "1 hour" (0=Never, 1=30min, 2=1h, 3=2h, 4=Custom)
            custom_minutes_input: String::new(),
            balance_input: String::new(),
            balance_error: None,
            schedule_field: ScheduleField::Schedule,
            value_planck: UNIT_PLANCK, // Default: 1 UNIT
        }
    }

    /// Create with pre-loaded config (custom dir from saved settings).
    pub fn new_with_config(custom_agent_dir: Option<String>) -> Self {
        let mut screen = Self::new();
        if let Some(dir) = custom_agent_dir {
            screen.use_embedded = false;
            screen.custom_dir_input = dir;
        }
        screen
    }

    pub fn reset(&mut self) {
        // Preserve the agent source selection
        let use_embedded = self.use_embedded;
        let custom_dir = self.custom_dir_input.clone();
        *self = Self::new();
        self.use_embedded = use_embedded;
        self.custom_dir_input = custom_dir;
    }

    /// Get the current agent source based on selection.
    pub fn agent_source(&self) -> AgentSource {
        if self.use_embedded {
            AgentSource::Embedded
        } else {
            AgentSource::Custom(self.custom_dir_input.clone())
        }
    }

    /// Validate the current agent source and cache the result.
    pub fn validate_source(&mut self) {
        let source = self.agent_source();
        self.source_validation = Some(source.validate());
    }

    pub async fn handle_key(
        &mut self,
        key: KeyCode,
        client: &ApiClient,
        _agent_dir: &str,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match self.step {
            CreateStep::SelectAgentSource => self.handle_select_source_key(key, tx.clone()),
            CreateStep::EnterAgentInfo => self.handle_agent_info_key(key, tx).await,
            CreateStep::WaitingClaim => {
                self.handle_waiting_claim_key(key, client.clone(), tx).await
            }
            CreateStep::ReviewSoul => self.handle_review_soul_key(key),
            CreateStep::ConfigureSchedule => {
                self.handle_configure_schedule_key(key, client.clone(), tx)
                    .await
            }
            CreateStep::Success => {
                if key == KeyCode::Enter || key == KeyCode::Esc {
                    return Ok(ScreenAction::GoHome);
                }
                Ok(ScreenAction::None)
            }
            _ => {
                if key == KeyCode::Esc {
                    return Ok(ScreenAction::GoHome);
                }
                Ok(ScreenAction::None)
            }
        }
    }

    fn handle_select_source_key(
        &mut self,
        key: KeyCode,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match key {
            KeyCode::Up | KeyCode::Down => {
                // Toggle between embedded and custom
                self.use_embedded = !self.use_embedded;
                self.error = None;
                // Re-validate when switching
                self.validate_source();
            }
            KeyCode::Tab => {
                // Switch to custom if on embedded, otherwise do nothing special
                if self.use_embedded {
                    self.use_embedded = false;
                    self.validate_source();
                }
            }
            KeyCode::Char(c) => {
                if !self.use_embedded {
                    self.custom_dir_input.push(c);
                    self.error = None;
                    // Validate as user types
                    self.validate_source();
                }
            }
            KeyCode::Backspace => {
                if !self.use_embedded {
                    self.custom_dir_input.pop();
                    self.error = None;
                    self.validate_source();
                }
            }
            KeyCode::Enter => {
                // Validate before proceeding
                self.validate_source();

                if let Some(ref validation) = self.source_validation {
                    if validation.is_valid() {
                        self.step = CreateStep::EnterAgentInfo;
                        self.error = None;

                        // Save the selection to config
                        let custom_dir = if self.use_embedded {
                            None
                        } else {
                            Some(self.custom_dir_input.clone())
                        };
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(AppMessage::AgentSourceSelected { custom_dir }).await;
                        });
                    } else {
                        self.error = Some("moltbook_agent.ship is required".to_string());
                    }
                } else {
                    // No validation yet, do it now
                    self.validate_source();
                    if let Some(ref validation) = self.source_validation {
                        if validation.is_valid() {
                            self.step = CreateStep::EnterAgentInfo;
                            self.error = None;

                            // Save the selection to config
                            let custom_dir = if self.use_embedded {
                                None
                            } else {
                                Some(self.custom_dir_input.clone())
                            };
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(AppMessage::AgentSourceSelected { custom_dir }).await;
                            });
                        } else {
                            self.error = Some("moltbook_agent.ship is required".to_string());
                        }
                    }
                }
            }
            KeyCode::Esc => {
                return Ok(ScreenAction::GoHome);
            }
            _ => {}
        }
        Ok(ScreenAction::None)
    }

    async fn handle_agent_info_key(
        &mut self,
        key: KeyCode,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match key {
            KeyCode::Tab => {
                // Cycle through fields: Name → Description → ApiKey → Name
                self.active_field = match self.active_field {
                    AgentInfoField::Name => AgentInfoField::Description,
                    AgentInfoField::Description => AgentInfoField::ApiKey,
                    AgentInfoField::ApiKey => AgentInfoField::Name,
                };
            }
            KeyCode::Up => {
                // Cycle backwards
                self.active_field = match self.active_field {
                    AgentInfoField::Name => AgentInfoField::ApiKey,
                    AgentInfoField::Description => AgentInfoField::Name,
                    AgentInfoField::ApiKey => AgentInfoField::Description,
                };
            }
            KeyCode::Down => {
                // Cycle forwards
                self.active_field = match self.active_field {
                    AgentInfoField::Name => AgentInfoField::Description,
                    AgentInfoField::Description => AgentInfoField::ApiKey,
                    AgentInfoField::ApiKey => AgentInfoField::Name,
                };
            }
            KeyCode::Char(c) => match self.active_field {
                AgentInfoField::Name => {
                    self.agent_name.push(c);
                    self.name_error = None;
                }
                AgentInfoField::Description => {
                    self.agent_description.push(c);
                }
                AgentInfoField::ApiKey => {
                    self.api_key_input.push(c);
                    self.api_key_error = None;
                    self.api_key_status = None;
                }
            },
            KeyCode::Backspace => match self.active_field {
                AgentInfoField::Name => {
                    self.agent_name.pop();
                }
                AgentInfoField::Description => {
                    self.agent_description.pop();
                }
                AgentInfoField::ApiKey => {
                    self.api_key_input.pop();
                    self.api_key_error = None;
                    self.api_key_status = None;
                }
            },
            KeyCode::Enter => {
                // If in API key field with input but NOT yet validated, validate it
                if self.active_field == AgentInfoField::ApiKey
                    && !self.api_key_input.is_empty()
                    && self.moltbook_api_key.is_none()
                {
                    self.api_key_status = Some("Validating...".to_string());
                    self.api_key_error = None;

                    let api_key = self.api_key_input.clone();
                    tokio::spawn(async move {
                        match crate::moltbook::get_agent_info(&api_key).await {
                            Ok(info) => {
                                let _ = tx
                                    .send(AppMessage::ApiKeyValidated {
                                        api_key,
                                        name: info.name,
                                        description: info.description,
                                        is_claimed: info.is_claimed,
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::ApiKeyInvalid(e.to_string())).await;
                            }
                        }
                    });
                }
                // If we have name + description (either entered or from API key), proceed
                else if !self.agent_name.is_empty() && !self.agent_description.is_empty() {
                    // If we already have a validated API key, skip registration and claim
                    if let Some(api_key) = &self.moltbook_api_key {
                        // Already have API key from validation - store agent on our server
                        self.step = CreateStep::RegisteringMoltbook; // Show loading state
                        let api_key = api_key.clone();
                        let name = self.agent_name.clone();

                        // We need to send a message to store the agent, which will happen
                        // via the ApiKeyStoreRequest flow. For now, send a special message.
                        tokio::spawn(async move {
                            // Signal that we have a pre-validated API key and need to store
                            let _ = tx
                                .send(AppMessage::ApiKeyReadyToStore { api_key, name })
                                .await;
                        });
                    } else {
                        // Need to register new agent
                        self.name_error = None;
                        self.error = None;
                        self.step = CreateStep::RegisteringMoltbook;

                        let name = self.agent_name.clone();
                        let description = self.agent_description.clone();
                        tokio::spawn(async move {
                            match crate::moltbook::register_agent(&name, &description).await {
                                Ok(resp) => {
                                    let _ = tx
                                        .send(AppMessage::MoltbookRegistered {
                                            api_key: resp.api_key,
                                            claim_url: resp.claim_url,
                                            verification_code: resp.verification_code,
                                        })
                                        .await;
                                }
                                Err(crate::moltbook::MoltbookError::NameTaken(msg)) => {
                                    let _ = tx.send(AppMessage::NameTaken(msg)).await;
                                }
                                Err(e) => {
                                    let _ = tx
                                        .send(AppMessage::RegistrationFailed(e.to_string()))
                                        .await;
                                }
                            }
                        });
                    }
                } else if self.agent_name.is_empty() {
                    self.name_error = Some("Name is required".to_string());
                    self.active_field = AgentInfoField::Name;
                } else {
                    self.error = Some("Description is required".to_string());
                    self.active_field = AgentInfoField::Description;
                }
            }
            KeyCode::Esc => {
                return Ok(ScreenAction::GoHome);
            }
            _ => {}
        }
        Ok(ScreenAction::None)
    }

    async fn handle_waiting_claim_key(
        &mut self,
        key: KeyCode,
        client: ApiClient,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match key {
            KeyCode::Char('o') | KeyCode::Char('O') => {
                // Open claim URL in browser
                if let Some(url) = &self.claim_url {
                    let _ = open::that(url);
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Check claim status using the API key
                if let Some(api_key) = &self.moltbook_api_key {
                    let api_key = api_key.clone();
                    let name = self.agent_name.clone();
                    tokio::spawn(async move {
                        // First check if claimed
                        match client.get_moltbook_status(&api_key).await {
                            Ok(resp) if resp.claimed => {
                                // Claimed! Now store the agent on our server
                                match client.store_agent(&name, &api_key).await {
                                    Ok(store_resp) => {
                                        let _ = tx
                                            .send(AppMessage::MoltbookClaimed {
                                                agent_id: store_resp.agent_id,
                                            })
                                            .await;
                                    }
                                    Err(e) => {
                                        let _ = tx
                                            .send(AppMessage::Error(format!(
                                                "Failed to store agent: {}",
                                                e
                                            )))
                                            .await;
                                    }
                                }
                            }
                            Ok(_) => {
                                let _ = tx
                                    .send(AppMessage::Error(
                                        "Not claimed yet. Complete the Twitter verification."
                                            .to_string(),
                                    ))
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::Error(e.to_string())).await;
                            }
                        }
                    });
                }
            }
            KeyCode::Esc => {
                return Ok(ScreenAction::GoHome);
            }
            _ => {}
        }
        Ok(ScreenAction::None)
    }

    fn handle_review_soul_key(&mut self, key: KeyCode) -> Result<ScreenAction> {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.step = CreateStep::ConfigureSchedule;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Open SOUL.md in editor (only for custom directory)
                if let AgentSource::Custom(dir) = self.agent_source() {
                    let soul_path = std::path::Path::new(&dir).join("SOUL.md");
                    if soul_path.exists() {
                        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
                        let _ = std::process::Command::new(&editor).arg(&soul_path).status();
                    }
                }
                // For embedded, editing is not supported (show message handled in render)
            }
            KeyCode::Esc => {
                return Ok(ScreenAction::GoHome);
            }
            _ => {}
        }
        Ok(ScreenAction::None)
    }

    async fn handle_configure_schedule_key(
        &mut self,
        key: KeyCode,
        client: ApiClient,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match self.schedule_field {
            ScheduleField::Schedule => match key {
                KeyCode::Up => {
                    if self.selected_schedule > 0 {
                        self.selected_schedule -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.selected_schedule < 4 {
                        self.selected_schedule += 1;
                    }
                }
                KeyCode::Tab => {
                    if self.selected_schedule == 4 {
                        self.schedule_field = ScheduleField::CustomMinutes;
                    } else {
                        self.schedule_field = ScheduleField::Balance;
                    }
                }
                KeyCode::Enter => {
                    self.schedule_field = ScheduleField::Balance;
                }
                KeyCode::Esc => {
                    return Ok(ScreenAction::GoHome);
                }
                _ => {}
            },
            ScheduleField::CustomMinutes => match key {
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    self.custom_minutes_input.push(c);
                }
                KeyCode::Backspace => {
                    self.custom_minutes_input.pop();
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.schedule_field = ScheduleField::Balance;
                }
                KeyCode::Up => {
                    self.schedule_field = ScheduleField::Schedule;
                }
                KeyCode::Esc => {
                    return Ok(ScreenAction::GoHome);
                }
                _ => {}
            },
            ScheduleField::Balance => match key {
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    if c == '.' && self.balance_input.contains('.') {
                        // Don't allow multiple decimal points
                    } else {
                        self.balance_input.push(c);
                        self.balance_error = None;
                    }
                }
                KeyCode::Backspace => {
                    self.balance_input.pop();
                    self.balance_error = None;
                }
                KeyCode::Tab | KeyCode::Up => {
                    if self.selected_schedule == 4 {
                        self.schedule_field = ScheduleField::CustomMinutes;
                    } else {
                        self.schedule_field = ScheduleField::Schedule;
                    }
                }
                KeyCode::Enter => {
                    // Compute schedule_option based on selection
                    self.schedule_option = match self.selected_schedule {
                        0 => None,      // Never
                        1 => Some(300), // 30 min
                        2 => Some(600), // 1 hour
                        3 => Some(1200), // 2 hours
                        4 => {
                            // Custom: parse minutes input
                            if let Ok(minutes) = self.custom_minutes_input.parse::<u32>() {
                                if minutes > 0 {
                                    // Convert minutes to blocks (10 blocks per minute at 6s/block)
                                    Some(minutes * 10)
                                } else {
                                    self.error = Some("Minutes must be greater than 0".to_string());
                                    self.schedule_field = ScheduleField::CustomMinutes;
                                    return Ok(ScreenAction::None);
                                }
                            } else {
                                self.error = Some("Enter valid minutes".to_string());
                                self.schedule_field = ScheduleField::CustomMinutes;
                                return Ok(ScreenAction::None);
                            }
                        }
                        _ => Some(600),
                    };

                    // Parse and validate balance
                    self.value_planck = self.parse_balance_to_planck();
                    if let Err(e) = self.validate_balance(&client).await {
                        self.balance_error = Some(e);
                        return Ok(ScreenAction::None);
                    }

                    self.step = CreateStep::Compiling;
                    self.start_compilation(client, tx).await?;
                }
                KeyCode::Esc => {
                    return Ok(ScreenAction::GoHome);
                }
                _ => {}
            },
        }
        Ok(ScreenAction::None)
    }

    fn parse_balance_to_planck(&self) -> u128 {
        if self.balance_input.is_empty() {
            return UNIT_PLANCK; // Default: 1 UNIT (existential deposit)
        }
        
        let input = self.balance_input.trim();
        if let Ok(decimal) = input.parse::<f64>() {
            (decimal * UNIT_PLANCK as f64) as u128
        } else {
            UNIT_PLANCK
        }
    }

    async fn validate_balance(&self, _client: &ApiClient) -> Result<(), String> {
        let value_planck = self.parse_balance_to_planck();
        
        // Skip validation if no balance input (will use default)
        if self.balance_input.is_empty() {
            return Ok(());
        }

        // We need wallet address to check balance - this will be available in app context
        // For now, just validate that the amount is reasonable (> 0 and parseable)
        if value_planck == 0 {
            return Err("Balance must be greater than 0".to_string());
        }

        Ok(())
    }

    async fn start_compilation(
        &mut self,
        client: ApiClient,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<()> {
        let source = self.agent_source();

        // Read files from the selected source (embedded or custom directory)
        let ship_file = source.read_file("moltbook_agent.ship").unwrap_or_default();
        let soul_md = source.read_file("SOUL.md").unwrap_or_default();
        let skill_md = source.read_file("SKILL.md").unwrap_or_default();
        let heartbeat_md = source.read_file("HEARTBEAT.md").unwrap_or_default();

        let agent_id = self.agent_id.clone().unwrap_or_default();
        let schedule = self.schedule_option;

        tokio::spawn(async move {
            match client
                .compile(
                    &agent_id,
                    &ship_file,
                    &soul_md,
                    &skill_md,
                    &heartbeat_md,
                    schedule,
                )
                .await
            {
                Ok(resp) if resp.success => {
                    if let Some(hex) = resp.compiled_hex {
                        let _ = tx.send(AppMessage::CompileDone { compiled_hex: hex }).await;
                    } else {
                        let _ = tx
                            .send(AppMessage::CompileFailed("No output".to_string()))
                            .await;
                    }
                }
                Ok(resp) => {
                    let errors = resp.errors.join("\n");
                    let _ = tx.send(AppMessage::CompileFailed(errors)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::CompileFailed(e.to_string())).await;
                }
            }
        });

        Ok(())
    }

    pub fn handle_moltbook_registered(
        &mut self,
        api_key: String,
        claim_url: String,
        verification_code: String,
    ) {
        self.moltbook_api_key = Some(api_key);
        self.claim_url = Some(claim_url);
        self.verification_code = Some(verification_code);
        self.step = CreateStep::WaitingClaim;
    }

    pub fn handle_name_taken(&mut self, message: &str) {
        // Go back to agent info step with name error (description is preserved)
        self.step = CreateStep::EnterAgentInfo;
        self.active_field = AgentInfoField::Name;
        self.name_error = Some(message.to_string());
    }

    pub fn handle_registration_failed(&mut self, message: &str) {
        // Go back to agent info step with general error (name and description preserved)
        self.step = CreateStep::EnterAgentInfo;
        self.error = Some(message.to_string());
    }

    pub fn handle_api_key_validated(
        &mut self,
        api_key: String,
        name: String,
        description: String,
        is_claimed: bool,
    ) {
        // Store the validated API key and populate fields
        self.moltbook_api_key = Some(api_key);
        self.agent_name = name;
        self.agent_description = description;
        self.api_key_status = Some("Valid! Press Enter to continue.".to_string());
        self.api_key_error = None;

        // If agent is claimed, we can skip the Twitter verification step
        if is_claimed {
            self.api_key_status =
                Some("Valid (already claimed)! Press Enter to continue.".to_string());
        }
    }

    pub fn handle_api_key_invalid(&mut self, message: &str) {
        self.api_key_error = Some(message.to_string());
        self.api_key_status = None;
        self.moltbook_api_key = None; // Clear any previously validated key
    }

    pub fn handle_moltbook_claimed(&mut self, agent_id: String) {
        self.agent_id = Some(agent_id);
        self.step = CreateStep::ReviewSoul;
    }

    pub fn handle_compile_done(&mut self, compiled_hex: String) {
        self.compiled_hex = Some(compiled_hex);
        self.step = CreateStep::Deploying;
        // Deployment needs to be triggered by calling start_deployment
    }

    /// Start the deployment process after compilation is done.
    /// This should be called from app.rs after CompileDone is handled.
    pub fn start_deployment(
        &self,
        client: ApiClient,
        wallet: WalletConfig,
        tx: mpsc::Sender<AppMessage>,
    ) {
        let compiled_hex = match &self.compiled_hex {
            Some(hex) => hex.clone(),
            None => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(AppMessage::DeployFailed("No compiled hex".to_string()))
                        .await;
                });
                return;
            }
        };

        let signer_address = wallet.public_key.clone();
        let value_planck = self.value_planck;

        // Generate a random salt
        let mut salt = [0u8; 32];
        let _ = getrandom::getrandom(&mut salt);
        let salt_hex = format!("0x{}", hex::encode(&salt));

        tokio::spawn(async move {
            // Step 1: Build the extrinsic (get call data from server)
            let build_result = match client
                .build_deploy(&compiled_hex, &salt_hex, &signer_address, value_planck)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(format!("Build failed: {}", e)))
                        .await;
                    return;
                }
            };

            // Step 2: Decode the call data and metadata
            let call_data = match hex::decode(build_result.call_data_hex.trim_start_matches("0x")) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(format!(
                            "Invalid call data: {}",
                            e
                        )))
                        .await;
                    return;
                }
            };

            let genesis_hash = match hex::decode(build_result.genesis_hash.trim_start_matches("0x"))
            {
                Ok(d) if d.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&d);
                    arr
                }
                _ => {
                    let _ = tx
                        .send(AppMessage::DeployFailed("Invalid genesis hash".to_string()))
                        .await;
                    return;
                }
            };

            // Step 3: Get the keypair for signing
            let keypair = match wallet.keypair() {
                Ok(k) => k,
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(format!("Wallet error: {}", e)))
                        .await;
                    return;
                }
            };

            // Step 4: Build and sign the extrinsic
            let signed_hex = match extrinsic::build_signed_extrinsic(
                &call_data,
                build_result.nonce,
                &genesis_hash,
                build_result.spec_version,
                build_result.transaction_version,
                &keypair,
            ) {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(format!("Signing failed: {}", e)))
                        .await;
                    return;
                }
            };

            // Step 5: Submit the extrinsic
            let submit_result = match client.submit_extrinsic(&signed_hex).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(format!("Submit failed: {}", e)))
                        .await;
                    return;
                }
            };

            // Step 6: Parse the AgentRegistered event to get the agent address
            let agent_address = extrinsic::parse_agent_registered_event(&submit_result.events);

            match agent_address {
                Some(addr) => {
                    let _ = tx
                        .send(AppMessage::DeployDone {
                            agent_address: addr,
                        })
                        .await;
                }
                None => {
                    let _ = tx
                        .send(AppMessage::DeployFailed(
                            "Could not find AgentRegistered event".to_string(),
                        ))
                        .await;
                }
            }
        });
    }

    pub fn handle_compile_failed(&mut self, error: &str) {
        self.error = Some(error.to_string());
        self.step = CreateStep::ConfigureSchedule;
    }

    pub fn handle_deploy_done(&mut self, agent_address: String) {
        self.agent_address = Some(agent_address);
        self.step = CreateStep::Success;
    }

    pub fn handle_deploy_failed(&mut self, error: &str) {
        self.error = Some(error.to_string());
        self.step = CreateStep::Compiling;
    }
}

impl Screen for CreateScreen {
    fn render(&self, frame: &mut Frame, area: Rect, _app: &App) {
        // Use more footer space when there's an error to display
        let footer_height = if self.error.is_some() { 4 } else { 2 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),             // Title bar
                Constraint::Min(10),               // Content
                Constraint::Length(footer_height), // Footer (larger when error)
            ])
            .split(area);

        // Title bar with step indicator
        let (step_num, step_name) = match self.step {
            CreateStep::SelectAgentSource => (1, "Agent Files"),
            CreateStep::EnterAgentInfo => (2, "Agent Info"),
            CreateStep::RegisteringMoltbook => (2, "Registering..."),
            CreateStep::WaitingClaim => (3, "Twitter Verification"),
            CreateStep::ReviewSoul => (4, "Review SOUL.md"),
            CreateStep::ConfigureSchedule => (5, "Configure Schedule"),
            CreateStep::Compiling => (6, "Compiling"),
            CreateStep::Deploying => (7, "Deploying"),
            CreateStep::Success => (7, "Complete"),
        };

        let progress = format!("Step {} of 7", step_num);
        let title_line = Line::from(vec![
            Span::styled(
                " CREATE AGENT ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(step_name, Style::default().fg(Color::LightRed)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(progress, Style::default().fg(Color::DarkGray)),
        ]);

        let title = Paragraph::new(title_line)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(title, chunks[0]);

        // Content based on step
        match self.step {
            CreateStep::SelectAgentSource => self.render_select_agent_source(frame, chunks[1]),
            CreateStep::EnterAgentInfo => self.render_agent_info(frame, chunks[1]),
            CreateStep::RegisteringMoltbook => {
                self.render_loading(frame, chunks[1], "Registering with Moltbook...")
            }
            CreateStep::WaitingClaim => self.render_waiting_claim(frame, chunks[1]),
            CreateStep::ReviewSoul => self.render_review_soul(frame, chunks[1]),
            CreateStep::ConfigureSchedule => self.render_configure_schedule(frame, chunks[1]),
            CreateStep::Compiling => {
                self.render_loading(frame, chunks[1], "Compiling SHIP code...")
            }
            CreateStep::Deploying => {
                self.render_loading(frame, chunks[1], "Deploying to Theseus chain...")
            }
            CreateStep::Success => self.render_success(frame, chunks[1]),
        }

        // Footer
        let footer = if let Some(err) = &self.error {
            // Show error with wrapping for long messages
            Paragraph::new(format!(" ✗ {}", err))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true })
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Cancel", Style::default().fg(Color::DarkGray)),
            ]))
            .alignment(Alignment::Center)
        };

        frame.render_widget(footer, chunks[2]);
    }
}

impl CreateScreen {
    fn render_select_agent_source(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2),  // Help text
                Constraint::Length(7),  // Options box
                Constraint::Length(1),  // Spacer
                Constraint::Length(3),  // Path input (for custom)
                Constraint::Length(1),  // Spacer
                Constraint::Length(6),  // File status
                Constraint::Length(2),  // Hint
                Constraint::Min(0),     // Remaining
            ])
            .split(area);

        // Help text
        let help = Paragraph::new("Select where to load agent files from:")
            .style(Style::default().fg(Color::White));
        frame.render_widget(help, chunks[0]);

        // Options
        let embedded_selected = self.use_embedded;
        let embedded_prefix = if embedded_selected { "● " } else { "○ " };
        let custom_prefix = if !embedded_selected { "● " } else { "○ " };

        let embedded_style = if embedded_selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let custom_style = if !embedded_selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let options = vec![
            ListItem::new(Line::from(vec![
                Span::styled(embedded_prefix, embedded_style),
                Span::styled("Use built-in defaults", embedded_style),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "Pre-configured agent files embedded in the binary",
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(vec![
                Span::styled(custom_prefix, custom_style),
                Span::styled("Use custom directory", custom_style),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "Load files from a local directory (for advanced users)",
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
        ];

        let list = List::new(options).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Agent Source ", Style::default().fg(Color::White))),
        );
        frame.render_widget(list, chunks[1]);

        // Path input (only active for custom)
        let path_active = !self.use_embedded;
        let path_border = if path_active { Color::Cyan } else { Color::DarkGray };
        let path_cursor = if path_active { "│" } else { "" };
        let path_text = if self.custom_dir_input.is_empty() && !path_active {
            "(select custom directory above to enter path)".to_string()
        } else {
            format!("{}{}", self.custom_dir_input, path_cursor)
        };
        let path_style = if path_active { Color::Cyan } else { Color::DarkGray };

        let path_input = Paragraph::new(path_text)
            .style(Style::default().fg(path_style))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(path_border))
                    .title(Span::styled(" Directory Path ", Style::default().fg(Color::White))),
            );
        frame.render_widget(path_input, chunks[3]);

        // File status
        let validation = self.source_validation.as_ref();
        let file_status_lines = if let Some(v) = validation {
            vec![
                self.format_file_status("moltbook_agent.ship", &v.ship_file, true),
                self.format_file_status("SOUL.md", &v.soul_md, false),
                self.format_file_status("SKILL.md", &v.skill_md, false),
                self.format_file_status("HEARTBEAT.md", &v.heartbeat_md, false),
            ]
        } else if self.use_embedded {
            // For embedded, show all as present (they're guaranteed)
            vec![
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(Color::Green)),
                    Span::styled("moltbook_agent.ship", Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(Color::Green)),
                    Span::styled("SOUL.md", Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(Color::Green)),
                    Span::styled("SKILL.md", Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(Color::Green)),
                    Span::styled("HEARTBEAT.md", Style::default().fg(Color::Green)),
                ]),
            ]
        } else {
            vec![Line::from(Span::styled(
                "Enter a directory path above",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let file_status = Paragraph::new(file_status_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Files ", Style::default().fg(Color::White))),
        );
        frame.render_widget(file_status, chunks[5]);

        // Hint
        let hint = Line::from(vec![
            Span::styled("[↑↓] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Switch option", Style::default().fg(Color::DarkGray)),
            Span::styled("  [Enter] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Continue", Style::default().fg(Color::DarkGray)),
        ]);
        let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
        frame.render_widget(hint_p, chunks[6]);
    }

    fn format_file_status<'a>(&self, name: &'a str, status: &FileStatus, _required: bool) -> Line<'a> {
        match status {
            FileStatus::Present => Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green)),
                Span::styled(name, Style::default().fg(Color::Green)),
            ]),
            FileStatus::Missing => Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                Span::styled(name, Style::default().fg(Color::Yellow)),
                Span::styled(" (missing - will use empty)", Style::default().fg(Color::DarkGray)),
            ]),
            FileStatus::RequiredMissing => Line::from(vec![
                Span::styled("✗ ", Style::default().fg(Color::Red)),
                Span::styled(name, Style::default().fg(Color::Red)),
                Span::styled(" (required!)", Style::default().fg(Color::Red)),
            ]),
        }
    }

    fn render_agent_info(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // Name label
                Constraint::Length(3), // Name input
                Constraint::Length(1), // Name error (if any)
                Constraint::Length(1), // Description label
                Constraint::Length(3), // Description input
                Constraint::Length(1), // Spacer
                Constraint::Length(1), // Separator
                Constraint::Length(1), // API key label
                Constraint::Length(3), // API key input
                Constraint::Length(1), // API key status/error
                Constraint::Length(1), // Spacer
                Constraint::Length(1), // Hint
                Constraint::Min(0),    // Remaining space
            ])
            .split(area);

        // Name label
        let name_label = Paragraph::new("Agent Name:").style(Style::default().fg(Color::White));
        frame.render_widget(name_label, chunks[0]);

        // Name input
        let name_active = self.active_field == AgentInfoField::Name;
        let name_border_color = if name_active {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let name_cursor = if name_active { "│" } else { "" };
        let name_style = if self.moltbook_api_key.is_some() {
            Color::Green
        } else {
            Color::Cyan
        };
        let name_input = Paragraph::new(format!("{}{}", self.agent_name, name_cursor))
            .style(Style::default().fg(name_style))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(name_border_color)),
            );
        frame.render_widget(name_input, chunks[1]);

        // Name error (inline, below name field)
        if let Some(err) = &self.name_error {
            let error_line = Paragraph::new(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(Color::Red)),
                Span::styled(err.as_str(), Style::default().fg(Color::Red)),
            ]));
            frame.render_widget(error_line, chunks[2]);
        }

        // Description label
        let desc_label = Paragraph::new("Description (shown on Moltbook):")
            .style(Style::default().fg(Color::White));
        frame.render_widget(desc_label, chunks[3]);

        // Description input
        let desc_active = self.active_field == AgentInfoField::Description;
        let desc_border_color = if desc_active {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let desc_cursor = if desc_active { "│" } else { "" };
        let desc_style = if self.moltbook_api_key.is_some() {
            Color::Green
        } else {
            Color::Cyan
        };
        let desc_input = Paragraph::new(format!("{}{}", self.agent_description, desc_cursor))
            .style(Style::default().fg(desc_style))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(desc_border_color)),
            );
        frame.render_widget(desc_input, chunks[4]);

        // Separator
        let separator = Paragraph::new("─────── or use existing API key ───────")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(separator, chunks[6]);

        // API key label
        let api_label = Paragraph::new("Moltbook API Key (paste with Ctrl+V / Cmd+V):")
            .style(Style::default().fg(Color::White));
        frame.render_widget(api_label, chunks[7]);

        // API key input
        let api_active = self.active_field == AgentInfoField::ApiKey;
        let api_border_color = if api_active {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let api_cursor = if api_active { "│" } else { "" };
        // Mask the API key for display (show first 15 chars + ...)
        let display_key = if self.api_key_input.len() > 20 {
            format!("{}...{}", &self.api_key_input[..15], api_cursor)
        } else {
            format!("{}{}", self.api_key_input, api_cursor)
        };
        let api_input = Paragraph::new(display_key)
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(api_border_color)),
            );
        frame.render_widget(api_input, chunks[8]);

        // API key status/error
        if let Some(status) = &self.api_key_status {
            let status_line = Paragraph::new(Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green)),
                Span::styled(status.as_str(), Style::default().fg(Color::Green)),
            ]));
            frame.render_widget(status_line, chunks[9]);
        } else if let Some(err) = &self.api_key_error {
            let error_line = Paragraph::new(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(Color::Red)),
                Span::styled(err.as_str(), Style::default().fg(Color::Red)),
            ]));
            frame.render_widget(error_line, chunks[9]);
        }

        // Hint
        let hint = Line::from(vec![
            Span::styled("[Tab] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Switch field", Style::default().fg(Color::DarkGray)),
            Span::styled("  [Enter] ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if self.moltbook_api_key.is_some() {
                    "Continue"
                } else {
                    "Register / Validate"
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
        frame.render_widget(hint_p, chunks[11]);
    }

    fn render_loading(&self, frame: &mut Frame, area: Rect, message: &str) {
        let _spinner = "◐◓◑◒";
        let loading_lines = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled("⏳", Style::default().fg(Color::Yellow))),
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(Color::White))),
            Line::from(""),
            Line::from(Span::styled(
                "Please wait...",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let loading = Paragraph::new(loading_lines).alignment(Alignment::Center);
        frame.render_widget(loading, area);
    }

    fn render_waiting_claim(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(5), // Code display
                Constraint::Length(1), // Spacer
                Constraint::Min(6),    // Instructions
            ])
            .split(area);

        // Verification code display
        let code_display = if let Some(code) = &self.verification_code {
            vec![
                Line::from(Span::styled(
                    "Verification Code",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    code.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let code_box = Paragraph::new(code_display)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(code_box, chunks[0]);

        // Instructions
        let instructions = vec![
            Line::from(vec![
                Span::styled(
                    " [O] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Open claim URL in browser",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [C] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Check verification status",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Post the code on Twitter, then verify on Moltbook",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let inst_box = Paragraph::new(instructions).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Instructions ",
                    Style::default().fg(Color::White),
                )),
        );
        frame.render_widget(inst_box, chunks[2]);
    }

    fn render_review_soul(&self, frame: &mut Frame, area: Rect) {
        let source = self.agent_source();
        let soul_content = source
            .read_file("SOUL.md")
            .unwrap_or_else(|| "Could not read SOUL.md".to_string());

        let preview: String = soul_content.lines().take(12).collect::<Vec<_>>().join("\n");

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(8), Constraint::Length(3)])
            .split(area);

        let content = Paragraph::new(preview)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(
                        " SOUL.md Preview ",
                        Style::default().fg(Color::White),
                    )),
            );
        frame.render_widget(content, chunks[0]);

        // Edit option only available for custom directory
        let can_edit = matches!(source, AgentSource::Custom(_));
        let options = if can_edit {
            Line::from(vec![
                Span::styled(
                    " [Y] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("Continue", Style::default().fg(Color::White)),
                Span::styled("    ", Style::default()),
                Span::styled(
                    " [E] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("Edit in $EDITOR", Style::default().fg(Color::White)),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    " [Y] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("Continue", Style::default().fg(Color::White)),
                Span::styled("    ", Style::default()),
                Span::styled(
                    "(using embedded defaults)",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        };
        let options_p = Paragraph::new(options).alignment(Alignment::Center);
        frame.render_widget(options_p, chunks[1]);
    }

    fn render_configure_schedule(&self, frame: &mut Frame, area: Rect) {
        let options = vec![
            "Never (only runs when prompted)",
            "Every 30 minutes",
            "Every 1 hour",
            "Every 2 hours",
            "Custom",
        ];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2),  // Help text
                Constraint::Length(9),  // Schedule options + custom input
                Constraint::Length(1),  // Spacer
                Constraint::Length(5),  // Balance section
                Constraint::Length(1),  // Balance error
                Constraint::Length(3),  // Info text
                Constraint::Length(2),  // Hint
                Constraint::Min(0),     // Remaining
            ])
            .split(area);

        let help = Paragraph::new("How often should your agent check in?")
            .style(Style::default().fg(Color::White));
        frame.render_widget(help, chunks[0]);

        // Build schedule options with custom minutes input inline
        let schedule_active = self.schedule_field == ScheduleField::Schedule;
        let schedule_border = if schedule_active { Color::Cyan } else { Color::DarkGray };
        
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let is_selected = i == self.selected_schedule;
                let (prefix, style) = if is_selected {
                    (
                        "● ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("○ ", Style::default().fg(Color::White))
                };
                
                // For custom option, show the input field inline
                if i == 4 {
                    let custom_active = self.schedule_field == ScheduleField::CustomMinutes;
                    let cursor = if custom_active { "│" } else { "" };
                    let input_style = if custom_active {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    
                    ListItem::new(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled("Custom: ", style),
                        Span::styled(
                            format!("{}{}", self.custom_minutes_input, cursor),
                            input_style,
                        ),
                        Span::styled(" minutes", Style::default().fg(Color::DarkGray)),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(*label, style),
                    ]))
                }
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(schedule_border))
                .title(Span::styled(
                    " Schedule ",
                    Style::default().fg(Color::White),
                )),
        );
        frame.render_widget(list, chunks[1]);

        // Balance input section
        let balance_active = self.schedule_field == ScheduleField::Balance;
        let balance_border = if balance_active { Color::Cyan } else { Color::DarkGray };
        let balance_cursor = if balance_active { "│" } else { "" };
        
        let balance_display = if self.balance_input.is_empty() {
            format!("1.0{} (default)", balance_cursor)
        } else {
            format!("{}{}", self.balance_input, balance_cursor)
        };
        
        let balance_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(3)])
            .split(chunks[3]);
            
        let balance_label = Paragraph::new("Initial balance for agent (in UNITS):")
            .style(Style::default().fg(Color::White));
        frame.render_widget(balance_label, balance_chunks[0]);
        
        let balance_input = Paragraph::new(balance_display)
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(balance_border)),
            );
        frame.render_widget(balance_input, balance_chunks[1]);

        // Balance error
        if let Some(err) = &self.balance_error {
            let error_line = Paragraph::new(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(Color::Red)),
                Span::styled(err.as_str(), Style::default().fg(Color::Red)),
            ]));
            frame.render_widget(error_line, chunks[4]);
        }

        // Info text about scheduled runs
        let info_text = if self.selected_schedule == 0 {
            "Agent will only run when you prompt it manually."
        } else {
            "Scheduled runs cost gas. Ensure agent has enough balance."
        };
        let info = Paragraph::new(vec![
            Line::from(Span::styled(info_text, Style::default().fg(Color::Yellow))),
            Line::from(Span::styled(
                "Tip: Keep some balance in your wallet for future deployments.",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .wrap(Wrap { trim: true });
        frame.render_widget(info, chunks[5]);

        let hint = Line::from(vec![
            Span::styled("[↑↓] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Navigate", Style::default().fg(Color::DarkGray)),
            Span::styled("  [Tab] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Switch field", Style::default().fg(Color::DarkGray)),
            Span::styled("  [Enter] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Deploy", Style::default().fg(Color::DarkGray)),
        ]);
        let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
        frame.render_widget(hint_p, chunks[6]);
    }

    fn render_success(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // Success header
                Constraint::Length(4), // Address
                Constraint::Min(3),    // Message
            ])
            .split(area);

        // Success header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("✓ ", Style::default().fg(Color::Green)),
            Span::styled(
                "AGENT DEPLOYED SUCCESSFULLY",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(header, chunks[0]);

        // Address display
        if let Some(addr) = &self.agent_address {
            let addr_lines = vec![
                Line::from(Span::styled(
                    "Agent Address",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(addr.clone(), Style::default().fg(Color::Cyan))),
            ];
            let addr_box = Paragraph::new(addr_lines)
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                );
            frame.render_widget(addr_box, chunks[1]);
        }

        // Continue message
        let msg = Paragraph::new(Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::White)),
            Span::styled(" to continue", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, chunks[2]);
    }
}
