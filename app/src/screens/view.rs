//! View agent details screen.

use crate::{
    app::{App, AppMessage, ScreenAction},
    client::{AgentInfo, ApiClient, MoltbookPost},
    screens::Screen,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use tokio::sync::mpsc;

pub struct ViewScreen {
    pub agent_info: Option<AgentInfo>,
    pub posts: Vec<MoltbookPost>,
    pub loading: bool,
    pub error: Option<String>,
}

impl ViewScreen {
    pub fn new() -> Self {
        Self {
            agent_info: None,
            posts: Vec::new(),
            loading: false,
            error: None,
        }
    }

    pub fn reset(&mut self) {
        self.agent_info = None;
        self.posts.clear();
        self.loading = true;
        self.error = None;
    }

    pub fn handle_key(
        &mut self,
        key: KeyCode,
        client: &ApiClient,
        agent_address: Option<&str>,
        tx: mpsc::Sender<AppMessage>,
    ) -> Result<ScreenAction> {
        match key {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Refresh data
                if let Some(addr) = agent_address {
                    self.loading = true;
                    self.error = None;
                    Self::fetch_data(client.clone(), addr.to_string(), tx);
                }
            }
            KeyCode::Esc => {
                return Ok(ScreenAction::GoHome);
            }
            _ => {}
        }
        Ok(ScreenAction::None)
    }

    /// Start fetching agent data (called when entering the screen).
    pub fn start_fetch(&mut self, client: ApiClient, agent_address: String, tx: mpsc::Sender<AppMessage>) {
        self.loading = true;
        self.error = None;
        Self::fetch_data(client, agent_address, tx);
    }

    fn fetch_data(client: ApiClient, agent_address: String, tx: mpsc::Sender<AppMessage>) {
        let addr = agent_address.clone();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        
        tokio::spawn(async move {
            // Fetch agent info
            match client_clone.get_agent(&addr).await {
                Ok(info) => {
                    let _ = tx_clone.send(AppMessage::AgentInfoFetched { info }).await;
                }
                Err(e) => {
                    let _ = tx_clone.send(AppMessage::FetchFailed(format!("Agent info: {}", e))).await;
                }
            }
        });

        tokio::spawn(async move {
            // Fetch posts
            match client.get_posts(&agent_address).await {
                Ok(resp) => {
                    let _ = tx.send(AppMessage::PostsFetched { posts: resp.posts }).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FetchFailed(format!("Posts: {}", e))).await;
                }
            }
        });
    }

    pub fn handle_agent_info(&mut self, info: AgentInfo) {
        self.agent_info = Some(info);
        self.check_loading_done();
    }

    pub fn handle_posts(&mut self, posts: Vec<MoltbookPost>) {
        self.posts = posts;
        self.check_loading_done();
    }

    pub fn handle_fetch_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    fn check_loading_done(&mut self) {
        // Stop loading once we have both info and posts (or error)
        if self.agent_info.is_some() && !self.posts.is_empty() {
            self.loading = false;
        }
        // Also stop if agent_info came back but no posts means loading should stop
        if self.agent_info.is_some() {
            self.loading = false;
        }
    }
}

impl Screen for ViewScreen {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),   // Title bar
                Constraint::Length(5),   // Agent info
                Constraint::Min(8),      // Posts
                Constraint::Length(2),   // Footer
            ])
            .split(area);

        // Title bar
        let title_line = Line::from(vec![
            Span::styled(" AGENT DETAILS ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if self.loading { "Loading..." } else { "Ready" },
                Style::default().fg(if self.loading { Color::Yellow } else { Color::Green }),
            ),
        ]);

        let title = Paragraph::new(title_line)
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(title, chunks[0]);

        // Agent info card (only show if authenticated)
        let mut info_lines = vec![];

        if let Some(name) = app.agent_name() {
            info_lines.push(Line::from(vec![
                Span::styled("  Name    ", Style::default().fg(Color::DarkGray)),
                Span::styled(name, Style::default().fg(Color::White)),
            ]));
        }

        if let Some(addr) = app.agent_address() {
            let short = if addr.len() > 40 {
                format!("{}...{}", &addr[..16], &addr[addr.len() - 12..])
            } else {
                addr.to_string()
            };
            info_lines.push(Line::from(vec![
                Span::styled("  Address ", Style::default().fg(Color::DarkGray)),
                Span::styled(short, Style::default().fg(Color::Cyan)),
            ]));
        }

        info_lines.push(Line::from(vec![
            Span::styled("  Status  ", Style::default().fg(Color::DarkGray)),
            Span::styled("● Active", Style::default().fg(Color::Green)),
        ]));

        let info = Paragraph::new(info_lines)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Agent ", Style::default().fg(Color::White))));
        frame.render_widget(info, chunks[1]);

        // Posts section
        if self.loading {
            let loading = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("⏳ Loading posts...", Style::default().fg(Color::Yellow))),
            ])
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Recent Posts ", Style::default().fg(Color::White))));
            frame.render_widget(loading, chunks[2]);
        } else if self.posts.is_empty() {
            let empty = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("No posts yet", Style::default().fg(Color::DarkGray))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[R]", Style::default().fg(Color::White)),
                    Span::styled(" to refresh", Style::default().fg(Color::DarkGray)),
                ]),
            ])
            .alignment(Alignment::Center)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Recent Posts ", Style::default().fg(Color::White))));
            frame.render_widget(empty, chunks[2]);
        } else {
            let items: Vec<ListItem> = self
                .posts
                .iter()
                .take(5)  // Limit displayed posts
                .map(|p| {
                    let submolt = p.submolt.as_ref().map(|s| s.name.as_str()).unwrap_or("general");
                    // Use title if available, otherwise content
                    let text = p.title.as_deref()
                        .or(p.content.as_deref())
                        .unwrap_or("");
                    let preview = if text.len() > 70 {
                        format!("{}...", &text[..70])
                    } else {
                        text.to_string()
                    };
                    let votes = format!("↑{}", p.upvotes);
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(format!("  m/{} ", submolt), Style::default().fg(Color::LightRed)),
                            Span::styled("• ", Style::default().fg(Color::DarkGray)),
                            Span::styled(votes, Style::default().fg(Color::Green)),
                            Span::styled(" • ", Style::default().fg(Color::DarkGray)),
                            Span::styled(&p.created_at, Style::default().fg(Color::DarkGray)),
                        ]),
                        Line::from(Span::styled(format!("  {}", preview), Style::default().fg(Color::White))),
                        Line::from(""),
                    ])
                })
                .collect();

            let list = List::new(items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(
                        format!(" Recent Posts ({}) ", self.posts.len()),
                        Style::default().fg(Color::White),
                    )));
            frame.render_widget(list, chunks[2]);
        }

        // Footer
        let footer_content = if let Some(err) = &self.error {
            Line::from(vec![
                Span::styled(" ✗ ", Style::default().fg(Color::Red)),
                Span::styled(err.as_str(), Style::default().fg(Color::Red)),
            ])
        } else {
            Line::from(vec![
                Span::styled("[R] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Refresh", Style::default().fg(Color::DarkGray)),
                Span::styled("  [Esc] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Back", Style::default().fg(Color::DarkGray)),
            ])
        };

        let footer = Paragraph::new(footer_content).alignment(Alignment::Center);
        frame.render_widget(footer, chunks[3]);
    }
}
