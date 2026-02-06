//! Prompt agent screen with chat-style UI.

use crate::{
    app::{App, AppMessage, ScreenAction},
    client::{ApiClient, ChatMessage, ChainEventData},
    config::AppConfig,
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
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum PromptStep {
    EnterPrompt,
    Submitting,
    Running,
    Complete,
}

/// Status of running tools
#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub name: String,
    pub completed: bool,
}

pub struct PromptScreen {
    pub step: PromptStep,
    pub input_buffer: String,
    pub run_id: Option<u64>,
    /// Accumulated chat messages from the conversation
    pub chat_messages: Vec<ChatMessage>,
    /// Currently running or recently completed tools
    pub tool_status: Vec<ToolStatus>,
    /// Final output from the agent
    pub final_output: Option<String>,
    /// Status messages for UI feedback
    pub status_messages: Vec<String>,
    /// Error message if any
    pub error: Option<String>,
    /// Show detailed tool call/result data (toggle with 'd')
    pub detailed_view: bool,
    /// Scroll offset for conversation view
    pub scroll_offset: u16,
}

impl PromptScreen {
    pub fn new() -> Self {
        Self {
            step: PromptStep::EnterPrompt,
            input_buffer: String::new(),
            run_id: None,
            chat_messages: Vec::new(),
            tool_status: Vec::new(),
            final_output: None,
            status_messages: Vec::new(),
            error: None,
            detailed_view: true, // Show full details by default
            scroll_offset: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Scroll up by n lines
    fn scroll_up(&mut self, n: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll down by n lines (bounded by content height)
    fn scroll_down(&mut self, n: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(n);
        // Will be bounded in render based on actual content height
    }

    pub async fn handle_key(
        &mut self,
        key: KeyCode,
        config: &AppConfig,
        client: &ApiClient,
        wallet: Option<&WalletConfig>,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match self.step {
            PromptStep::EnterPrompt => {
                match key {
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    }
                    KeyCode::Enter if !self.input_buffer.is_empty() => {
                        // Check wallet exists
                        let wallet = match wallet {
                            Some(w) => w,
                            None => {
                                self.error = Some("No wallet available".to_string());
                                return Ok(ScreenAction::None);
                            }
                        };
                        
                        // Start submitting
                        let agent_address = match &config.agent_address {
                            Some(addr) => addr.clone(),
                            None => {
                                self.error = Some("No agent configured".to_string());
                                return Ok(ScreenAction::None);
                            }
                        };

                        self.step = PromptStep::Submitting;
                        self.status_messages.clear();
                        self.status_messages.push("Building extrinsic...".to_string());

                        // Start the submit flow
                        Self::start_prompt_submission(
                            client.clone(),
                            wallet.clone(),
                            agent_address,
                            self.input_buffer.clone(),
                            tx,
                        );
                    }
                    KeyCode::Esc => {
                        return Ok(ScreenAction::GoHome);
                    }
                    _ => {}
                }
            }
            PromptStep::Submitting | PromptStep::Running => {
                match key {
                    KeyCode::Char('d') => {
                        // Toggle detailed view
                        self.detailed_view = !self.detailed_view;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        // Scroll down
                        self.scroll_down(3);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        // Scroll up
                        self.scroll_up(3);
                    }
                    KeyCode::Esc => {
                        self.step = PromptStep::Complete;
                        self.error = Some("Cancelled by user (agent may still be running)".to_string());
                    }
                    _ => {}
                }
            }
            PromptStep::Complete => {
                match key {
                    KeyCode::Enter | KeyCode::Esc => {
                        return Ok(ScreenAction::GoHome);
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.scroll_down(3);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.scroll_up(3);
                    }
                    KeyCode::Char('d') => {
                        self.detailed_view = !self.detailed_view;
                    }
                    _ => {}
                }
            }
        }
        Ok(ScreenAction::None)
    }

    fn start_prompt_submission(
        client: ApiClient,
        wallet: WalletConfig,
        agent_address: String,
        input: String,
        tx: mpsc::Sender<AppMessage>,
    ) {
        let signer_address = wallet.public_key.clone();

        tokio::spawn(async move {
            // Step 1: Build the extrinsic
            let build_result = match client.build_call(&agent_address, &input, &signer_address).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(AppMessage::PromptFailed(format!("Build failed: {}", e))).await;
                    return;
                }
            };

            // Step 2: Decode the call data
            let call_data = match hex::decode(build_result.call_data_hex.trim_start_matches("0x")) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.send(AppMessage::PromptFailed(format!("Invalid call data: {}", e))).await;
                    return;
                }
            };

            let genesis_hash = match hex::decode(build_result.genesis_hash.trim_start_matches("0x")) {
                Ok(d) if d.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&d);
                    arr
                }
                _ => {
                    let _ = tx.send(AppMessage::PromptFailed("Invalid genesis hash".to_string())).await;
                    return;
                }
            };

            // Step 3: Get keypair
            let keypair = match wallet.keypair() {
                Ok(k) => k,
                Err(e) => {
                    let _ = tx.send(AppMessage::PromptFailed(format!("Wallet error: {}", e))).await;
                    return;
                }
            };

            let _ = tx.send(AppMessage::PromptStatus("Signing extrinsic...".to_string())).await;

            // Step 4: Sign
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
                    let _ = tx.send(AppMessage::PromptFailed(format!("Signing failed: {}", e))).await;
                    return;
                }
            };

            let _ = tx.send(AppMessage::PromptStatus("Submitting to chain...".to_string())).await;

            // Step 5: Submit
            let submit_result = match client.submit_extrinsic(&signed_hex).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(AppMessage::PromptFailed(format!("Submit failed: {}", e))).await;
                    return;
                }
            };

            // Step 6: Parse run_id from events
            let run_id = extrinsic::parse_agent_call_queued_event(&submit_result.events);
            
            match run_id {
                Some(id) => {
                    let _ = tx.send(AppMessage::PromptSubmitted { run_id: id }).await;
                    // Start streaming events
                    Self::stream_run_events(client, id, tx).await;
                }
                None => {
                    let _ = tx.send(AppMessage::PromptFailed(
                        "Could not find AgentCallQueued event".to_string()
                    )).await;
                }
            }
        });
    }

    async fn stream_run_events(
        client: ApiClient,
        run_id: u64,
        tx: mpsc::Sender<AppMessage>,
    ) {
        let _ = tx.send(AppMessage::PromptStatus(format!("Run ID: {} - Streaming events...", run_id))).await;

        // Get the SSE stream URL and start consuming events
        let url = format!("{}/chain/events/{}", client.base_url(), run_id);
        
        let http_client = reqwest::Client::new();
        let mut req = http_client.get(&url);
        
        if let Some(token) = client.auth_token() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(AppMessage::PromptFailed(format!("SSE connection failed: {}", e))).await;
                return;
            }
        };

        if !resp.status().is_success() {
            let _ = tx.send(AppMessage::PromptFailed(format!("SSE connection error: {}", resp.status()))).await;
            return;
        }

        // Use eventsource-stream to consume SSE events
        use eventsource_stream::Eventsource;
        use futures::StreamExt;

        let mut stream = resp.bytes_stream().eventsource();

        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    let data = event.data;

                    // Try to parse as structured event
                    match serde_json::from_str::<ChainEventData>(&data) {
                        Ok(chain_event) => {
                            // Send structured event to UI
                            let _ = tx.send(AppMessage::ChainEvent(chain_event.clone())).await;
                            
                            // Check if run completed
                            match chain_event {
                                ChainEventData::Completed { output, .. } => {
                                    let _ = tx.send(AppMessage::RunCompleted { result: output }).await;
                                    break;
                                }
                                ChainEventData::Failed { reason, .. } => {
                                    let _ = tx.send(AppMessage::PromptFailed(reason)).await;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Err(_) => {
                            // Fallback to raw event display
                            let _ = tx.send(AppMessage::PromptStatus(format!("[{}] {}", event.event, data))).await;
                            
                            // Check for error event type
                            if event.event == "error" {
                                let _ = tx.send(AppMessage::PromptFailed(data)).await;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::PromptFailed(format!("SSE error: {}", e))).await;
                    break;
                }
            }
        }
    }

    /// Handle a structured chain event
    pub fn handle_chain_event(&mut self, event: ChainEventData) {
        match event {
            ChainEventData::RunStarted { agent_name, .. } => {
                self.status_messages.push(format!("Agent '{}' started", agent_name));
            }
            ChainEventData::Messages { messages, .. } => {
                // Replace chat messages with new ones
                self.chat_messages = messages;
            }
            ChainEventData::ToolsStarted { tools, .. } => {
                // Add new tools to the list (accumulate, don't replace)
                for tool_name in tools {
                    // Only add if not already present
                    if !self.tool_status.iter().any(|t| t.name == tool_name) {
                        self.tool_status.push(ToolStatus {
                            name: tool_name,
                            completed: false,
                        });
                    }
                }
            }
            ChainEventData::ToolsCompleted { tools, .. } => {
                // Mark matching tools as completed
                for status in &mut self.tool_status {
                    if tools.contains(&status.name) {
                        status.completed = true;
                    }
                }
            }
            ChainEventData::WaitingForInput { reason, .. } => {
                self.status_messages.push(format!("Waiting: {}", reason));
            }
            ChainEventData::Resumed { .. } => {
                self.status_messages.push("Run resumed".to_string());
            }
            ChainEventData::Routing { result, next_node, .. } => {
                if let Some(node) = next_node {
                    self.status_messages.push(format!("Routing: {} -> node {}", result, node));
                }
            }
            ChainEventData::Completed { output, .. } => {
                self.final_output = Some(output);
            }
            ChainEventData::Failed { reason, .. } => {
                self.error = Some(reason);
            }
            ChainEventData::Raw { variant, data } => {
                self.status_messages.push(format!("[{}] {}", variant, 
                    if data.len() > 50 { format!("{}...", &data[..50]) } else { data }));
            }
        }
    }

    /// Handle a status message (non-structured)
    pub fn handle_status_message(&mut self, msg: String) {
        self.status_messages.push(msg);
        // Keep only last 10 status messages
        if self.status_messages.len() > 10 {
            self.status_messages.remove(0);
        }
    }

    pub fn handle_prompt_submitted(&mut self, run_id: u64) {
        self.run_id = Some(run_id);
        self.step = PromptStep::Running;
        self.status_messages.push(format!("Submitted! Run ID: {}", run_id));
    }

    pub fn handle_run_completed(&mut self, result: String) {
        self.step = PromptStep::Complete;
        self.final_output = Some(result);
    }

    pub fn handle_prompt_failed(&mut self, error: String) {
        self.step = PromptStep::Complete;
        self.error = Some(error);
    }

    /// Render the chat-style view of messages (scrollable, filtered)
    fn render_chat_view(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        // User's initial prompt
        if !self.input_buffer.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  You", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]));
            // Show prompt (truncated if long)
            let prompt_lines: Vec<&str> = self.input_buffer.lines().collect();
            for line in prompt_lines.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(line.to_string(), Style::default().fg(Color::White)),
                ]));
            }
            if prompt_lines.len() > 4 {
                lines.push(Line::from(Span::styled("  │ ...", Style::default().fg(Color::DarkGray))));
            }
            lines.push(Line::from(""));
        }

        // Filter messages: only show tool calls, tool results, and final response
        // Skip: system messages, user messages (already shown above), intermediate assistant text
        let assistant_messages: Vec<_> = self.chat_messages.iter()
            .filter(|m| matches!(m, ChatMessage::Assistant { .. }))
            .collect();
        let last_assistant_idx = assistant_messages.len().saturating_sub(1);

        for msg in self.chat_messages.iter() {
            match msg {
                ChatMessage::System { .. } | ChatMessage::User { .. } => {
                    // Skip - system is internal, user prompt already shown
                }
                ChatMessage::Assistant { content, tool_calls, output } => {
                    // Only show if: has tool calls OR is the last assistant message (final response)
                    let is_last = assistant_messages.get(last_assistant_idx)
                        .map(|m| std::ptr::eq(*m, msg))
                        .unwrap_or(false);
                    let has_tools = !tool_calls.is_empty();

                    if has_tools {
                        // Show tool calls
                        for tc in tool_calls {
                            let (icon, icon_color) = self.get_tool_status_icon(&tc.name);
                            // Get descriptive action based on tool name + arguments
                            let action_desc = Self::describe_tool_action(&tc.name, &tc.arguments);

                            lines.push(Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                                Span::styled(action_desc, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                            ]));

                            // Show relevant params (filter out api_key, endpoint)
                            if self.detailed_view {
                                let arg_lines = Self::format_tool_args(&tc.arguments);
                                for line in arg_lines {
                                    lines.push(line);
                                }
                            }
                        }
                    }

                    // Show final response text (only for last assistant message without tool calls)
                    if is_last && !has_tools {
                        if let Some(text) = content {
                            if !text.is_empty() {
                                lines.push(Line::from(""));
                                lines.push(Line::from(vec![
                                    Span::styled("  Agent", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                                ]));
                                for line in text.lines() {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                                        Span::styled(line.to_string(), Style::default().fg(Color::White)),
                                    ]));
                                }
                            }
                        }
                    }

                    // Show output if present
                    if let Some(out) = output {
                        if !out.is_empty() {
                            lines.push(Line::from(vec![
                                Span::styled("  → ", Style::default().fg(Color::Green)),
                                Span::styled(Self::truncate_string(out, 60), Style::default().fg(Color::Green)),
                            ]));
                        }
                    }
                }
                ChatMessage::ToolResult { .. } => {
                    // Don't show tool results as separate lines - status is shown via icon on the tool call
                }
            }
        }

        // Show minimal status only when no tool info yet
        if self.step == PromptStep::Submitting {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("◐ ", Style::default().fg(Color::Yellow)),
                Span::styled("Submitting transaction...", Style::default().fg(Color::Yellow)),
            ]));
        } else if self.step == PromptStep::Running && self.chat_messages.is_empty() && self.tool_status.is_empty() {
            // Only show "thinking" if we have no info yet
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("◐ ", Style::default().fg(Color::Magenta)),
                Span::styled("Agent is thinking...", Style::default().fg(Color::Magenta)),
            ]));
        }

        // Calculate scroll bounds
        let content_height = lines.len() as u16;
        let view_height = area.height.saturating_sub(2); // account for borders
        let is_scrollable = content_height > view_height;

        // Bound scroll offset (can't exceed max)
        let max_scroll = content_height.saturating_sub(view_height);
        let scroll_offset = self.scroll_offset.min(max_scroll);

        // Show scroll indicator in title if scrollable
        let title = if is_scrollable {
            " Conversation [j/k scroll] ".to_string()
        } else {
            " Conversation ".to_string()
        };

        let content = Paragraph::new(lines)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(title, Style::default().fg(Color::White))))
            .scroll((scroll_offset, 0));

        frame.render_widget(content, area);
    }

    /// Get a human-friendly tool status icon
    fn get_tool_status_icon(&self, tool_name: &str) -> (&'static str, Color) {
        self.tool_status.iter()
            .find(|s| s.name == tool_name)
            .map(|s| if s.completed { ("✓", Color::Green) } else { ("◐", Color::Yellow) })
            .unwrap_or(("○", Color::DarkGray))
    }

    /// Truncate a string with ellipsis
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len.saturating_sub(3)])
        }
    }

    /// Generate human-readable action description from tool name and arguments
    fn describe_tool_action(tool_name: &str, arguments: &str) -> String {
        // Parse arguments to get endpoint
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(arguments);
        let endpoint = parsed.as_ref().ok()
            .and_then(|v| v.get("endpoint"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match tool_name {
            "moltbook_get" => {
                // Map endpoint to human-readable description
                match endpoint {
                    "agents/me" => "Fetching agent profile".to_string(),
                    "feed" => "Getting agent feed".to_string(),
                    e if e.starts_with("posts/") => "Fetching post details".to_string(),
                    e if e.starts_with("submolts/") && e.contains("/posts") => "Getting submolt posts".to_string(),
                    e if e.starts_with("submolts/") => "Fetching submolt info".to_string(),
                    e if e.starts_with("users/") => "Fetching user profile".to_string(),
                    _ => format!("Fetching {}", if endpoint.is_empty() { "data" } else { endpoint }),
                }
            }
            "moltbook_post" => {
                match endpoint {
                    "posts" => "Creating post".to_string(),
                    "comments" => "Adding comment".to_string(),
                    e if e.contains("upvote") => "Upvoting".to_string(),
                    e if e.contains("downvote") => "Downvoting".to_string(),
                    _ => format!("Posting to {}", if endpoint.is_empty() { "moltbook" } else { endpoint }),
                }
            }
            "moltbook_comment" => "Adding comment".to_string(),
            "moltbook_upvote" => "Upvoting".to_string(),
            "moltbook_downvote" => "Downvoting".to_string(),
            "moltbook_search" => "Searching Moltbook".to_string(),
            "http_get" => "Fetching external data".to_string(),
            "http_post" => "Sending HTTP request".to_string(),
            _ => tool_name.replace('_', " "),
        }
    }

    /// Format tool arguments for display - only show relevant fields (params/body), skip api_key/endpoint
    fn format_tool_args(arguments: &str) -> Vec<Line<'static>> {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(arguments);
        let mut lines = Vec::new();

        if let Ok(serde_json::Value::Object(map)) = parsed {
            // Check if there's a body (for POST) or params (for GET)
            if let Some(body) = map.get("body") {
                if let serde_json::Value::Object(body_map) = body {
                    let field_count = body_map.len();
                    for (i, (key, value)) in body_map.iter().enumerate() {
                        let is_last = i == field_count - 1;
                        let prefix = if is_last { "    └─ " } else { "    ├─ " };
                        let formatted_value = Self::format_json_value(value, 50);
                        
                        lines.push(Line::from(vec![
                            Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                            Span::styled(format!("{}: ", key), Style::default().fg(Color::Cyan)),
                            Span::styled(formatted_value, Style::default().fg(Color::White)),
                        ]));
                    }
                }
            }
            
            if let Some(params) = map.get("params") {
                if let serde_json::Value::Object(params_map) = params {
                    let field_count = params_map.len();
                    for (i, (key, value)) in params_map.iter().enumerate() {
                        let is_last = i == field_count - 1;
                        let prefix = if is_last { "    └─ " } else { "    ├─ " };
                        let formatted_value = Self::format_json_value(value, 50);
                        
                        lines.push(Line::from(vec![
                            Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                            Span::styled(format!("{}: ", key), Style::default().fg(Color::Cyan)),
                            Span::styled(formatted_value, Style::default().fg(Color::White)),
                        ]));
                    }
                }
            }
        }

        lines
    }

    /// Format a single JSON value for display
    fn format_json_value(value: &serde_json::Value, max_len: usize) -> String {
        match value {
            serde_json::Value::String(s) => {
                format!("\"{}\"", Self::truncate_string(s, max_len))
            }
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
            serde_json::Value::Object(_) => "{...}".to_string(),
        }
    }
}

impl Screen for PromptScreen {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // Title bar
                Constraint::Min(10),    // Content
                Constraint::Length(2),  // Footer
            ])
            .split(area);

        // Title bar
        let step_text = match self.step {
            PromptStep::EnterPrompt => "Enter Prompt",
            PromptStep::Submitting => "Submitting...",
            PromptStep::Running => "Running",
            PromptStep::Complete => "Complete",
        };
        
        let title_line = Line::from(vec![
            Span::styled(" PROMPT AGENT ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(step_text, Style::default().fg(Color::LightRed)),
        ]);

        let title = Paragraph::new(title_line)
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(title, chunks[0]);

        // Content
        match self.step {
            PromptStep::EnterPrompt => {
                let inner = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(2),  // Agent info
                        Constraint::Length(5),  // Input
                        Constraint::Min(1),     // Spacer
                    ])
                    .split(chunks[1]);

                // Agent info (only show if authenticated)
                let agent_info = if let Some(addr) = app.agent_address() {
                    let short = if addr.len() > 30 {
                        format!("{}...{}", &addr[..12], &addr[addr.len() - 8..])
                    } else {
                        addr.to_string()
                    };
                    format!("Target: {}", short)
                } else {
                    "No agent configured".to_string()
                };
                let info = Paragraph::new(agent_info)
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(info, inner[0]);

                // Input box
                let cursor = if self.input_buffer.is_empty() { "│" } else { "" };
                let input = Paragraph::new(format!("{}{}", self.input_buffer, cursor))
                    .style(Style::default().fg(Color::Cyan))
                    .block(Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                        .title(Span::styled(" Your Prompt ", Style::default().fg(Color::White))));
                frame.render_widget(input, inner[1]);
            }
            PromptStep::Submitting | PromptStep::Running => {
                self.render_chat_view(frame, chunks[1]);
            }
            PromptStep::Complete => {
                // Show the final chat view with completion status
                let inner = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(6),      // Chat messages
                        Constraint::Length(6),   // Final status
                    ])
                    .split(chunks[1]);

                // Show chat messages if any
                self.render_chat_view(frame, inner[0]);

                // Completion status box
                let (icon, header, header_color) = if self.error.is_some() {
                    ("✗", "Run Failed", Color::Red)
                } else {
                    ("✓", "Completed", Color::Green)
                };

                let mut status_lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(format!("  {} ", icon), Style::default().fg(header_color)),
                        Span::styled(header, Style::default().fg(header_color).add_modifier(Modifier::BOLD)),
                    ]),
                ];

                // Add result or error detail
                if let Some(output) = &self.final_output {
                    if !output.is_empty() {
                        // Clean up the output for display
                        let clean_output = output.trim();
                        let display = if clean_output.len() > 80 {
                            format!("{}...", &clean_output[..80])
                        } else {
                            clean_output.to_string()
                        };
                        status_lines.push(Line::from(vec![
                            Span::styled("    ", Style::default()),
                            Span::styled(display, Style::default().fg(Color::White)),
                        ]));
                    }
                } else if let Some(err) = &self.error {
                    let error_display = if err.len() > 70 {
                        format!("{}...", &err[..70])
                    } else {
                        err.clone()
                    };
                    status_lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(error_display, Style::default().fg(Color::Red)),
                    ]));
                }

                status_lines.push(Line::from(""));
                status_lines.push(Line::from(vec![
                    Span::styled("  Press ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[Enter]", Style::default().fg(Color::White)),
                    Span::styled(" to continue  ", Style::default().fg(Color::DarkGray)),
                ]));

                let status_p = Paragraph::new(status_lines)
                    .block(Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(Color::DarkGray)));
                frame.render_widget(status_p, inner[1]);
            }
        }

        // Footer
        let footer_content = match self.step {
            PromptStep::EnterPrompt => Line::from(vec![
                Span::styled("[Enter] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Send", Style::default().fg(Color::DarkGray)),
                Span::styled("  [Esc] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Cancel", Style::default().fg(Color::DarkGray)),
            ]),
            PromptStep::Submitting => Line::from(Span::styled(
                "Submitting to chain...",
                Style::default().fg(Color::Yellow),
            )),
            PromptStep::Running => {
                let detail_hint = if self.detailed_view { "Hide details" } else { "Show details" };
                Line::from(vec![
                    Span::styled("[j/k] ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Scroll", Style::default().fg(Color::DarkGray)),
                    Span::styled("  [d] ", Style::default().fg(Color::DarkGray)),
                    Span::styled(detail_hint, Style::default().fg(Color::DarkGray)),
                    Span::styled("  [Esc] ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Stop watching", Style::default().fg(Color::DarkGray)),
                ])
            }
            PromptStep::Complete => {
                let detail_hint = if self.detailed_view { "Hide details" } else { "Show details" };
                Line::from(vec![
                    Span::styled("[j/k] ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Scroll", Style::default().fg(Color::DarkGray)),
                    Span::styled("  [d] ", Style::default().fg(Color::DarkGray)),
                    Span::styled(detail_hint, Style::default().fg(Color::DarkGray)),
                    Span::styled("  [Enter] ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Continue", Style::default().fg(Color::DarkGray)),
                ])
            }
        };

        let footer = Paragraph::new(footer_content).alignment(Alignment::Center);
        frame.render_widget(footer, chunks[2]);
    }
}
