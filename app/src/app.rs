//! Application state machine.

use crate::{
    auth,
    client::ApiClient,
    config::AppConfig,
    screens::{
        create::CreateScreen, home::HomeScreen, prompt::PromptScreen, view::ViewScreen, Screen,
    },
    wallet::WalletConfig,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{layout::Rect, Frame};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use tokio::sync::mpsc;

/// Messages for async operations.
#[derive(Debug, Clone)]
pub enum AppMessage {
    /// Auth completed
    AuthCompleted(String),
    /// Auth failed
    AuthFailed(String),
    /// Wallet funded
    WalletFunded,
    /// Wallet funding failed
    WalletFundFailed(String),
    /// Balance updated
    BalanceUpdated(String),
    /// Moltbook registered (from direct TUI call to Moltbook API)
    MoltbookRegistered { api_key: String, claim_url: String, verification_code: String },
    /// Moltbook registration failed (any error)
    RegistrationFailed(String),
    /// Agent name already taken - need to choose different name
    NameTaken(String),
    /// Existing API key validated - got agent info
    ApiKeyValidated { api_key: String, name: String, description: String, is_claimed: bool },
    /// API key validation failed
    ApiKeyInvalid(String),
    /// Ready to store agent with existing API key (skip registration)
    ApiKeyReadyToStore { api_key: String, name: String },
    /// Moltbook claimed - agent stored on server
    MoltbookClaimed { agent_id: String },
    /// Compilation done
    CompileDone { compiled_hex: String },
    /// Compilation failed
    CompileFailed(String),
    /// Deployment done
    DeployDone { agent_address: String },
    /// Deployment failed
    DeployFailed(String),
    /// Prompt submitted, now streaming
    PromptSubmitted { run_id: u64 },
    /// Structured chain event from agent run
    ChainEvent(crate::client::ChainEventData),
    /// Status message (non-structured feedback)
    PromptStatus(String),
    /// Agent run completed
    RunCompleted { result: String },
    /// Prompt failed
    PromptFailed(String),
    /// Agent info fetched
    AgentInfoFetched { info: crate::client::AgentInfo },
    /// Agent posts fetched
    PostsFetched { posts: Vec<crate::client::MoltbookPost> },
    /// Fetch failed
    FetchFailed(String),
    /// User's agent data restored from server
    AgentDataRestored { name: String, chain_address: String },
    /// Agent source selected (embedded or custom dir)
    AgentSourceSelected { custom_dir: Option<String> },
    /// Error occurred
    Error(String),
}

/// Application screen state.
#[derive(Debug, Clone, PartialEq)]
pub enum AppScreen {
    Home,
    EmailInput,  // Email entry for magic link
    Auth,        // Waiting for auth callback
    Create,
    Prompt,
    View,
}

/// Action returned from screen handlers.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenAction {
    None,
    GoHome,
}

/// Main application state.
pub struct App {
    pub config: AppConfig,
    pub wallet: Option<WalletConfig>,  // Created only after authentication
    pub client: ApiClient,
    pub agent_dir: String,
    pub screen: AppScreen,
    pub quit: bool,

    // Screen states
    pub home: HomeScreen,
    pub create: CreateScreen,
    pub prompt: PromptScreen,
    pub view: ViewScreen,

    // Transient state
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    
    // Email input for magic link auth
    pub email_input: String,
    
    // Wallet balance (formatted string)
    pub wallet_balance: Option<String>,

    // Image state for lobster banner
    pub lobster_image: Option<StatefulProtocol>,
}

impl App {
    pub async fn new(server_url: String, agent_dir: String) -> Result<Self> {
        // Load or create config
        let mut config = AppConfig::load().unwrap_or_default();
        config.server_url = server_url.clone();

        // Create API client
        let mut client = ApiClient::new(server_url);
        if let Some(token) = &config.auth_token {
            client.set_auth_token(token.clone());
        }

        // Only load wallet if user is authenticated (wallet is created after first auth)
        let wallet = if config.auth_token.is_some() {
            WalletConfig::load()?
        } else {
            None
        };

        // Try to load the lobster image
        let lobster_image = Self::load_lobster_image(&agent_dir);

        // Extract custom_agent_dir before moving config
        let custom_agent_dir = config.custom_agent_dir.clone();

        Ok(Self {
            config,
            wallet,
            client,
            agent_dir,
            screen: AppScreen::Home,
            quit: false,
            home: HomeScreen::new(),
            create: CreateScreen::new_with_config(custom_agent_dir),
            prompt: PromptScreen::new(),
            view: ViewScreen::new(),
            status_message: None,
            error_message: None,
            email_input: String::new(),
            wallet_balance: None,
            lobster_image,
        })
    }
    
    /// Ensure wallet exists (create if needed). Called after successful authentication.
    pub fn ensure_wallet(&mut self) -> Result<()> {
        if self.wallet.is_none() {
            let wallet = WalletConfig::load_or_generate()?;
            self.wallet = Some(wallet);
        }
        Ok(())
    }
    
    /// Get wallet address if authenticated and wallet exists.
    pub fn wallet_address(&self) -> Option<&str> {
        if self.config.is_authenticated() {
            self.wallet.as_ref().map(|w| w.public_key.as_str())
        } else {
            None
        }
    }
    
    /// Get short wallet address if authenticated and wallet exists.
    pub fn wallet_short_address(&self) -> Option<String> {
        if self.config.is_authenticated() {
            self.wallet.as_ref().map(|w| w.short_address())
        } else {
            None
        }
    }
    
    /// Get agent address only if authenticated (agent belongs to logged-in user).
    pub fn agent_address(&self) -> Option<&str> {
        if self.config.is_authenticated() {
            self.config.agent_address.as_deref()
        } else {
            None
        }
    }
    
    /// Get agent name only if authenticated.
    pub fn agent_name(&self) -> Option<&str> {
        if self.config.is_authenticated() {
            self.config.agent_name.as_deref()
        } else {
            None
        }
    }
    
    /// Check if user has an agent (only valid when authenticated).
    pub fn has_agent(&self) -> bool {
        self.config.is_authenticated() && self.config.agent_address.is_some()
    }

    /// Initialize the app after creation - validates persisted session and fetches balance.
    /// This should be called once after App::new() with the message sender.
    pub fn init_session(&self, tx: mpsc::Sender<AppMessage>) {
        if self.config.auth_token.is_some() {
            // We have a persisted token - validate it and fetch balance
            let client = self.client.clone();
            let wallet_address = self.wallet.as_ref().map(|w| w.public_key.clone());
            tokio::spawn(async move {
                // Try to get user info to validate the token
                match client.get_me().await {
                    Ok(_) => {
                        // Token is valid - fetch balance if we have a wallet
                        if let Some(addr) = wallet_address {
                            match client.get_balance(&addr).await {
                                Ok(resp) => {
                                    let _ = tx.send(AppMessage::BalanceUpdated(resp.balance_formatted)).await;
                                }
                                Err(_) => {
                                    // Balance fetch failed but session is valid
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Token is invalid/expired - notify to clear it
                        let _ = tx.send(AppMessage::AuthFailed("Session expired. Please login again.".to_string())).await;
                    }
                }
            });
        }
    }

    fn load_lobster_image(agent_dir: &str) -> Option<StatefulProtocol> {
        // Query terminal for graphics capabilities and font size
        // This automatically detects: Kitty, iTerm2, Sixel, or falls back to halfblocks
        // Note: Must be called AFTER entering alternate screen but BEFORE event loop
        let picker = match Picker::from_query_stdio() {
            Ok(p) => p,
            Err(_) => {
                // Fallback: use halfblocks with estimated font size
                // This works on ALL terminals but doesn't support transparency
                Picker::from_fontsize((8, 16))
            }
        };
        
        // Try multiple possible paths for the image
        let possible_paths = [
            format!("{}/pol.png", agent_dir),
            "pol.png".to_string(),
            "app/pol.png".to_string(),
        ];
        
        for path in &possible_paths {
            if let Ok(reader) = image::ImageReader::open(path) {
                if let Ok(dyn_img) = reader.decode() {
                    return Some(picker.new_resize_protocol(dyn_img));
                }
            }
        }
        
        None
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        match self.screen {
            AppScreen::Home => {
                // Use the special render function that handles the image
                crate::screens::home::render_home_with_image(frame, area, self);
            }
            AppScreen::EmailInput => self.render_email_input(frame, area),
            AppScreen::Auth => self.render_auth(frame, area),
            AppScreen::Create => self.create.render(frame, area, self),
            AppScreen::Prompt => self.prompt.render(frame, area, self),
            AppScreen::View => self.view.render(frame, area, self),
        }
    }

    fn render_email_input(&self, frame: &mut Frame, area: Rect) {
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph},
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Min(4),
                Constraint::Length(2),
            ])
            .split(area);

        // Title
        let title = Paragraph::new(Line::from(vec![
            Span::styled(" LOGIN ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Email Magic Link", Style::default().fg(Color::LightRed)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(title, chunks[0]);

        // Instructions
        let instructions = Paragraph::new("Enter your email address to receive a magic link:")
            .style(Style::default().fg(Color::White));
        frame.render_widget(instructions, chunks[1]);

        // Email input
        let cursor = if self.email_input.is_empty() { "│" } else { "" };
        let input = Paragraph::new(format!("{}{}", self.email_input, cursor))
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Email ", Style::default().fg(Color::White))));
        frame.render_widget(input, chunks[2]);

        // Help text
        let help = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "A magic link will be sent to your email.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "Click the link to complete authentication.",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(help, chunks[3]);

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[Enter] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Send", Style::default().fg(Color::DarkGray)),
            Span::styled("  [Esc] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Cancel", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(footer, chunks[4]);
    }

    fn render_auth(&self, frame: &mut Frame, area: Rect) {
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph},
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(area);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(" LOGIN ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Waiting for authentication...", Style::default().fg(Color::Yellow)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(title, chunks[0]);

        let message = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("⏳ Check your email for the magic link", Style::default().fg(Color::Yellow))),
            Line::from(""),
            Line::from(Span::styled("Click the link in your email to authenticate.", Style::default().fg(Color::White))),
            Line::from(Span::styled("This screen will update automatically when complete.", Style::default().fg(Color::DarkGray))),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Login"));
        frame.render_widget(message, chunks[1]);
    }

    pub async fn handle_key(&mut self, key: KeyCode, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        // Clear error message on any key
        self.error_message = None;

        match self.screen {
            AppScreen::Home => self.handle_home_key(key, tx).await,
            AppScreen::EmailInput => self.handle_email_input_key(key, tx).await,
            AppScreen::Auth => self.handle_auth_key(key),
            AppScreen::Create => {
                let action = self.create.handle_key(key, &self.client, &self.agent_dir, tx).await?;
                self.handle_screen_action(action);
                Ok(())
            }
            AppScreen::Prompt => {
                let action = self.prompt.handle_key(key, &self.config, &self.client, self.wallet.as_ref(), tx).await?;
                self.handle_screen_action(action);
                Ok(())
            }
            AppScreen::View => {
                let agent_addr = self.agent_address().map(|s| s.to_string());
                let action = self.view.handle_key(key, &self.client, agent_addr.as_deref(), tx)?;
                self.handle_screen_action(action);
                Ok(())
            }
        }
    }

    fn handle_screen_action(&mut self, action: ScreenAction) {
        match action {
            ScreenAction::None => {}
            ScreenAction::GoHome => {
                self.screen = AppScreen::Home;
            }
        }
    }

    async fn handle_home_key(&mut self, key: KeyCode, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        match key {
            KeyCode::Char('1') => {
                if !self.config.is_authenticated() {
                    // Navigate to email input screen
                    self.email_input.clear();
                    self.screen = AppScreen::EmailInput;
                } else {
                    self.screen = AppScreen::Create;
                    self.create.reset();
                }
            }
            KeyCode::Char('2') => {
                if !self.config.is_authenticated() {
                    // Twitter login - not yet implemented
                    self.status_message = Some("Twitter login coming soon! Use email login for now.".to_string());
                } else if self.config.has_agent() {
                    self.screen = AppScreen::Prompt;
                    self.prompt.reset();
                }
            }
            KeyCode::Char('3') if self.config.is_authenticated() && self.config.has_agent() => {
                self.screen = AppScreen::View;
                self.view.reset();
                // Start fetching data immediately (only if authenticated)
                if let Some(addr) = self.agent_address() {
                    self.view.start_fetch(self.client.clone(), addr.to_string(), tx.clone());
                }
            }
            KeyCode::Char('4') if self.config.is_authenticated() => {
                self.config.logout();
                self.config.save()?;
                self.client.clear_auth_token();
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_email_input_key(&mut self, key: KeyCode, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        match key {
            KeyCode::Char(c) => {
                self.email_input.push(c);
            }
            KeyCode::Backspace => {
                self.email_input.pop();
            }
            KeyCode::Enter if !self.email_input.is_empty() => {
                // Validate email format (basic check)
                if self.email_input.contains('@') && self.email_input.contains('.') {
                    self.start_email_auth(tx).await?;
                } else {
                    self.error_message = Some("Please enter a valid email address".to_string());
                }
            }
            KeyCode::Esc => {
                self.email_input.clear();
                self.screen = AppScreen::Home;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_auth_key(&mut self, key: KeyCode) -> Result<()> {
        if key == KeyCode::Esc {
            self.screen = AppScreen::Home;
        }
        Ok(())
    }

    async fn start_email_auth(&mut self, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        self.screen = AppScreen::Auth;
        self.status_message = Some("Sending magic link...".to_string());

        let server_url = self.config.server_url.clone();
        let email = self.email_input.clone();
        
        tokio::spawn(async move {
            match auth::run_oauth_flow(&server_url, auth::AuthMethod::Email(email)).await {
                Ok(token) => {
                    let _ = tx.send(AppMessage::AuthCompleted(token)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::AuthFailed(e.to_string())).await;
                }
            }
        });

        Ok(())
    }

    #[allow(dead_code)]
    async fn start_twitter_auth(&mut self, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        self.screen = AppScreen::Auth;
        self.status_message = Some("Opening browser for Twitter login...".to_string());

        let server_url = self.config.server_url.clone();
        tokio::spawn(async move {
            match auth::run_oauth_flow(&server_url, auth::AuthMethod::Twitter).await {
                Ok(token) => {
                    let _ = tx.send(AppMessage::AuthCompleted(token)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::AuthFailed(e.to_string())).await;
                }
            }
        });

        Ok(())
    }

    pub async fn handle_message(&mut self, msg: AppMessage, tx: mpsc::Sender<AppMessage>) -> Result<()> {
        match msg {
            AppMessage::AuthCompleted(token) => {
                self.config.auth_token = Some(token.clone());
                self.config.save()?;
                self.client.set_auth_token(token);
                self.screen = AppScreen::Home;
                self.status_message = Some("Logged in! Setting up wallet...".to_string());
                
                // Create wallet if it doesn't exist (first-time auth)
                if let Err(e) = self.ensure_wallet() {
                    self.error_message = Some(format!("Failed to create wallet: {}", e));
                    return Ok(());
                }
                
                // Check if wallet needs funding on-chain
                let client = self.client.clone();
                let wallet_address = self.wallet.as_ref().map(|w| w.public_key.clone()).unwrap_or_default();
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    match client.get_me().await {
                        Ok(me) if !me.has_wallet => {
                            // Wallet not funded yet, fund it
                            match client.fund_wallet(&wallet_address).await {
                                Ok(_) => {
                                    let _ = tx_clone.send(AppMessage::WalletFunded).await;
                                }
                                Err(e) => {
                                    let _ = tx_clone.send(AppMessage::WalletFundFailed(e.to_string())).await;
                                }
                            }
                        }
                        Ok(_) => {
                            // Wallet already funded
                            let _ = tx_clone.send(AppMessage::WalletFunded).await;
                        }
                        Err(e) => {
                            let _ = tx_clone.send(AppMessage::Error(format!("Failed to check wallet: {}", e))).await;
                        }
                    }
                });
            }
            AppMessage::AuthFailed(e) => {
                self.screen = AppScreen::Home;
                self.error_message = Some(format!("Auth failed: {}", e));
                // Clear invalid token
                self.config.auth_token = None;
                self.client.clear_auth_token();
                let _ = self.config.save();
            }
            AppMessage::WalletFunded => {
                self.status_message = Some("Logged in! Wallet ready.".to_string());
                // Fetch balance
                self.fetch_balance(tx.clone());
                // Also fetch user's agents to restore any existing agent data
                self.fetch_user_agents(tx.clone());
            }
            AppMessage::WalletFundFailed(e) => {
                self.error_message = Some(format!("Wallet funding failed: {}. You may need more tokens to deploy.", e));
                // Still try to fetch balance
                self.fetch_balance(tx.clone());
            }
            AppMessage::BalanceUpdated(balance) => {
                self.wallet_balance = Some(balance);
            }
            AppMessage::MoltbookRegistered { api_key, claim_url, verification_code } => {
                self.create.handle_moltbook_registered(api_key, claim_url, verification_code);
            }
            AppMessage::RegistrationFailed(msg) => {
                // Go back to agent info form with error
                self.create.handle_registration_failed(&msg);
            }
            AppMessage::NameTaken(msg) => {
                // Go back to name input with name-specific error
                self.create.handle_name_taken(&msg);
            }
            AppMessage::ApiKeyValidated { api_key, name, description, is_claimed } => {
                self.create.handle_api_key_validated(api_key, name, description, is_claimed);
            }
            AppMessage::ApiKeyInvalid(msg) => {
                self.create.handle_api_key_invalid(&msg);
            }
            AppMessage::ApiKeyReadyToStore { api_key, name } => {
                // Store existing agent on our server
                let client = self.client.clone();
                tokio::spawn(async move {
                    match client.store_agent(&name, &api_key).await {
                        Ok(resp) => {
                            let _ = tx.send(AppMessage::MoltbookClaimed { 
                                agent_id: resp.agent_id 
                            }).await;
                        }
                        Err(e) => {
                            let _ = tx.send(AppMessage::RegistrationFailed(
                                format!("Failed to store agent: {}", e)
                            )).await;
                        }
                    }
                });
            }
            AppMessage::MoltbookClaimed { agent_id } => {
                self.create.handle_moltbook_claimed(agent_id);
            }
            AppMessage::CompileDone { compiled_hex } => {
                self.create.handle_compile_done(compiled_hex);
                // Start deployment immediately after compilation
                if let Some(wallet) = &self.wallet {
                    self.create.start_deployment(
                        self.client.clone(),
                        wallet.clone(),
                        tx.clone(),
                    );
                } else {
                    self.error_message = Some("No wallet available for deployment".to_string());
                }
            }
            AppMessage::CompileFailed(e) => {
                self.error_message = Some(format!("Compilation failed: {}", e));
                self.create.handle_compile_failed(&e);
            }
            AppMessage::DeployDone { agent_address } => {
                self.config.agent_address = Some(agent_address.clone());
                self.config.agent_name = Some(self.create.agent_name.clone());
                self.config.save()?;
                
                // Update the server with the chain address
                if let Some(agent_id) = self.create.agent_id.clone() {
                    let client = self.client.clone();
                    let addr = agent_address.clone();
                    tokio::spawn(async move {
                        // Best-effort update - deployment already succeeded
                        let _ = client.update_agent_address(&agent_id, &addr).await;
                    });
                }
                
                self.create.handle_deploy_done(agent_address);
            }
            AppMessage::DeployFailed(e) => {
                self.error_message = Some(format!("Deployment failed: {}", e));
                self.create.handle_deploy_failed(&e);
            }
            AppMessage::PromptSubmitted { run_id } => {
                self.prompt.handle_prompt_submitted(run_id);
            }
            AppMessage::ChainEvent(event) => {
                self.prompt.handle_chain_event(event);
            }
            AppMessage::PromptStatus(msg) => {
                self.prompt.handle_status_message(msg);
            }
            AppMessage::RunCompleted { result } => {
                self.prompt.handle_run_completed(result);
            }
            AppMessage::PromptFailed(e) => {
                self.prompt.handle_prompt_failed(e);
            }
            AppMessage::AgentInfoFetched { info } => {
                self.view.handle_agent_info(info);
            }
            AppMessage::PostsFetched { posts } => {
                self.view.handle_posts(posts);
            }
            AppMessage::FetchFailed(e) => {
                self.view.handle_fetch_error(e);
            }
            AppMessage::AgentDataRestored { name, chain_address } => {
                // Restore agent data from server (happens on login)
                self.config.agent_name = Some(name);
                self.config.agent_address = Some(chain_address);
                let _ = self.config.save();
            }
            AppMessage::AgentSourceSelected { custom_dir } => {
                // Save the agent source selection to config
                self.config.custom_agent_dir = custom_dir;
                let _ = self.config.save();
            }
            AppMessage::Error(e) => {
                self.error_message = Some(e);
            }
        }
        Ok(())
    }

    /// Fetch wallet balance in background.
    fn fetch_balance(&self, tx: mpsc::Sender<AppMessage>) {
        let Some(wallet) = &self.wallet else {
            return; // No wallet yet
        };
        
        let client = self.client.clone();
        let address = wallet.public_key.clone();
        
        tokio::spawn(async move {
            match client.get_balance(&address).await {
                Ok(resp) => {
                    let _ = tx.send(AppMessage::BalanceUpdated(resp.balance_formatted)).await;
                }
                Err(_) => {
                    // Silently ignore balance fetch errors
                }
            }
        });
    }
    
    /// Fetch user's agents from server to restore any existing agent data.
    fn fetch_user_agents(&self, tx: mpsc::Sender<AppMessage>) {
        let client = self.client.clone();
        
        tokio::spawn(async move {
            match client.list_agents().await {
                Ok(agents) => {
                    // Find the first agent with a chain_address (deployed agent)
                    if let Some(agent) = agents.into_iter().find(|a| a.chain_address.is_some()) {
                        if let Some(chain_address) = agent.chain_address {
                            let _ = tx.send(AppMessage::AgentDataRestored {
                                name: agent.name,
                                chain_address,
                            }).await;
                        }
                    }
                }
                Err(_) => {
                    // Silently ignore - user might not have any agents yet
                }
            }
        });
    }
    
    /// Periodic JWT validity check. Logs out if session is invalid.
    pub fn check_session_validity(&self, tx: mpsc::Sender<AppMessage>) {
        if !self.config.is_authenticated() {
            return;
        }
        
        let client = self.client.clone();
        tokio::spawn(async move {
            match client.get_me().await {
                Ok(_) => {
                    // Session is still valid
                }
                Err(_) => {
                    // Session expired or invalid - trigger logout
                    let _ = tx.send(AppMessage::AuthFailed("Session expired. Please login again.".to_string())).await;
                }
            }
        });
    }
    
    /// Periodic balance refresh (public, called from main loop).
    pub fn refresh_balance(&self, tx: mpsc::Sender<AppMessage>) {
        self.fetch_balance(tx);
    }

    pub fn can_quit(&self) -> bool {
        self.screen == AppScreen::Home
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }
}
