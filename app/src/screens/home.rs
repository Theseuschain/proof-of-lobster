//! Home screen with Proof of Lobster branding.

use crate::{app::App, screens::Screen};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding, Paragraph},
    Frame,
};
use ratatui_image::StatefulImage;

/// ASCII art title - single line style
const TITLE_LARGE: &str = r#"
 ███████╗ ██████╗  ██████╗  ██████╗ ███████╗     ██████╗ ███████╗    ██╗      ██████╗ ██████╗ ███████╗████████╗███████╗██████╗ 
 ██╔══██║██╔══██╗██╔═══██╗██╔═══██╗██╔════╝    ██╔═══██╗██╔════╝    ██║     ██╔═══██╗██╔══██╗██╔════╝╚══██╔══╝██╔════╝██╔══██╗
 ███████║██████╔╝██║   ██║██║   ██║█████╗      ██║   ██║█████╗      ██║     ██║   ██║██████╔╝███████╗   ██║   █████╗  ██████╔╝
 ██╔════╝██╔══██╗██║   ██║██║   ██║██╔══╝      ██║   ██║██╔══╝      ██║     ██║   ██║██╔══██╗╚════██║   ██║   ██╔══╝  ██╔══██╗
 ██║     ██║  ██║╚██████╔╝╚██████╔╝██║         ╚██████╔╝██║         ███████╗╚██████╔╝██████╔╝███████║   ██║   ███████╗██║  ██║
 ╚═╝     ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝          ╚═════╝ ╚═╝         ╚══════╝ ╚═════╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝  ╚═╝
"#;

/// Medium title for narrower terminals  
const TITLE_MEDIUM: &str = r#"
  ██████╗ ██████╗  ██████╗  ██████╗ ███████╗     ██████╗ ███████╗
  ██╔══██╗██╔══██╗██╔═══██╗██╔═══██╗██╔════╝    ██╔═══██╗██╔════╝
  ██████╔╝██████╔╝██║   ██║██║   ██║█████╗      ██║   ██║█████╗  
  ██╔═══╝ ██╔══██╗██║   ██║██║   ██║██╔══╝      ██║   ██║██╔══╝  
  ██║     ██║  ██║╚██████╔╝╚██████╔╝██║         ╚██████╔╝██║     
  ╚═╝     ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝          ╚═════╝ ╚═╝     

  ██╗      ██████╗ ██████╗ ███████╗████████╗███████╗██████╗ 
  ██║     ██╔═══██╗██╔══██╗██╔════╝╚══██╔══╝██╔════╝██╔══██╗
  ██║     ██║   ██║██████╔╝███████╗   ██║   █████╗  ██████╔╝
  ██║     ██║   ██║██╔══██╗╚════██║   ██║   ██╔══╝  ██╔══██╗
  ███████╗╚██████╔╝██████╔╝███████║   ██║   ███████╗██║  ██║
  ╚══════╝ ╚═════╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝  ╚═╝
"#;

/// Compact title for small terminals
const TITLE_COMPACT: &str = r#"
  ╔══════════════════════════════════════╗
  ║                                      ║
  ║     🦞  PROOF OF LOBSTER  🦞         ║
  ║                                      ║
  ║     Deploy AI Agents on Theseus      ║
  ║                                      ║
  ╚══════════════════════════════════════╝
"#;

/// Fallback ASCII art if image can't be loaded
const LOBSTER_ASCII: &str = r#"
        ██  ████        ████  ██        
      ██████▒▒▒▒██    ██▒▒▒▒██████      
      ██▒▒▒▒██▒▒▒▒██████▒▒▒▒██▒▒▒▒██    
      ██▒▒▒▒▒▒████▒▒▒▒▒▒████▒▒▒▒▒▒██    
        ██▒▒▒▒▒▒██▒▒██▒▒██▒▒▒▒▒▒██      
          ████████▒▒▒▒▒▒████████        
                ██▒▒██▒▒██              
              ██████▒▒██████            
                  ████████              
                ██▒▒████▒▒██            
                  ████████              
"#;

pub struct HomeScreen;

impl HomeScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for HomeScreen {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        // This is the fallback render - the main render is render_home_with_image
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(vec![
                Constraint::Length(14), // Header
                Constraint::Length(5),  // Status
                Constraint::Min(6),     // Menu
                Constraint::Length(2),  // Footer
            ])
            .split(area);

        // Header - ASCII fallback
        let header = Paragraph::new(LOBSTER_ASCII)
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(header, chunks[0]);

        render_status_menu_footer(frame, &chunks, app);
    }
}

/// Render the home screen with mutable access to app (for image state).
pub fn render_home_with_image(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(vec![
            Constraint::Length(16), // Banner header (image + title)
            Constraint::Length(5),  // Status
            Constraint::Min(6),     // Menu
            Constraint::Length(2),  // Footer
        ])
        .split(area);

    // Banner area - split horizontally: image left, title right
    if let Some(ref mut image_state) = app.lobster_image {
        let banner_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(34), // Image on left
                Constraint::Min(40),    // Title on right
            ])
            .split(chunks[0]);

        // Render the image on the left
        let image_area = Rect::new(
            banner_chunks[0].x + 1,
            banner_chunks[0].y,
            banner_chunks[0].width.saturating_sub(2).min(32),
            banner_chunks[0].height,
        );
        let image_widget = StatefulImage::default();
        frame.render_stateful_widget(image_widget, image_area, image_state);

        // Render ASCII title on the right - choose based on width
        let title_area = banner_chunks[1];
        let available_width = title_area.width as usize;

        let title_text = if available_width >= 120 {
            TITLE_LARGE
        } else if available_width >= 60 {
            TITLE_MEDIUM
        } else {
            TITLE_COMPACT
        };

        let title = Paragraph::new(title_text)
            .style(Style::default().fg(Color::LightRed))
            .alignment(Alignment::Left);
        frame.render_widget(title, title_area);
    } else {
        // Fallback: no image, show ASCII lobster + title side by side
        let fallback_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Length(44), Constraint::Min(40)])
            .split(chunks[0]);

        let lobster = Paragraph::new(LOBSTER_ASCII)
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(lobster, fallback_chunks[0]);

        let title = Paragraph::new(TITLE_COMPACT)
            .style(Style::default().fg(Color::LightRed))
            .alignment(Alignment::Left);
        frame.render_widget(title, fallback_chunks[1]);
    }

    render_status_menu_footer(frame, &chunks, app);
}

/// Helper to render the status, menu, and footer sections
fn render_status_menu_footer(frame: &mut Frame, chunks: &[Rect], app: &App) {
    // Status section
    let auth_icon = if app.config.is_authenticated() {
        "●"
    } else {
        "○"
    };
    let auth_text = if app.config.is_authenticated() {
        "Authenticated"
    } else {
        "Not logged in"
    };
    let auth_color = if app.config.is_authenticated() {
        Color::Green
    } else {
        Color::Yellow
    };

    // Only show agent info when authenticated
    let (agent_text, agent_color) = if app.config.is_authenticated() {
        if let Some(addr) = app.agent_address() {
            let short = if addr.len() > 20 {
                format!("{}...{}", &addr[..10], &addr[addr.len() - 8..])
            } else {
                addr.to_string()
            };
            (format!("● Agent: {}", short), Color::Green)
        } else {
            ("○ No agent deployed".to_string(), Color::DarkGray)
        }
    } else {
        // Not authenticated - don't show agent status
        ("".to_string(), Color::DarkGray)
    };

    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Status ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::horizontal(1));

    // Build status lines
    let mut status_lines = vec![Line::from(vec![
        Span::styled(format!("{} ", auth_icon), Style::default().fg(auth_color)),
        Span::styled(auth_text, Style::default().fg(auth_color)),
    ])];

    // Only show wallet if authenticated
    if let Some(wallet_short) = app.wallet_short_address() {
        status_lines.push(Line::from(vec![
            Span::styled("◈ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("Wallet: {}", wallet_short),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        // Show balance if available
        if let Some(balance) = &app.wallet_balance {
            status_lines.push(Line::from(Span::styled(
                format!("  Balance: {} THE", balance),
                Style::default().fg(Color::Yellow),
            )));
        } else {
            status_lines.push(Line::from(Span::styled(
                "  Balance: loading...".to_string(),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    // Only show agent line if authenticated
    if !agent_text.is_empty() {
        status_lines.push(Line::from(Span::styled(
            &agent_text,
            Style::default().fg(agent_color),
        )));
    }

    let status_content = Paragraph::new(status_lines).block(status_block);

    frame.render_widget(status_content, chunks[1]);

    // Menu section
    let menu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Menu ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::horizontal(1));

    let mut items = Vec::new();

    if !app.config.is_authenticated() {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " [1] ",
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Login with Email", Style::default().fg(Color::White)),
            Span::styled(" (magic link)", Style::default().fg(Color::DarkGray)),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled(" [2] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Login with Twitter", Style::default().fg(Color::DarkGray)),
            Span::styled(" (coming soon)", Style::default().fg(Color::DarkGray)),
        ])));
    } else {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " [1] ",
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Create New Agent", Style::default().fg(Color::White)),
        ])));

        if app.config.has_agent() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    " [2] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("Prompt Agent", Style::default().fg(Color::White)),
            ])));
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    " [3] ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("View Agent Details", Style::default().fg(Color::White)),
            ])));
        }

        items.push(ListItem::new(Line::from(vec![
            Span::styled(" [4] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Logout", Style::default().fg(Color::DarkGray)),
        ])));
    }

    let menu = List::new(items).block(menu_block);
    frame.render_widget(menu, chunks[2]);

    // Footer - status messages or help
    let footer_content = if let Some(err) = &app.error_message {
        Line::from(vec![
            Span::styled(" ✗ ", Style::default().fg(Color::Red)),
            Span::styled(err.as_str(), Style::default().fg(Color::Red)),
        ])
    } else if let Some(status) = &app.status_message {
        Line::from(vec![
            Span::styled(" ✓ ", Style::default().fg(Color::Green)),
            Span::styled(status.as_str(), Style::default().fg(Color::Green)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" [1-4] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Select option", Style::default().fg(Color::DarkGray)),
            Span::styled("  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Q] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Quit", Style::default().fg(Color::DarkGray)),
        ])
    };

    let footer = Paragraph::new(footer_content).alignment(Alignment::Center);
    frame.render_widget(footer, chunks[3]);
}
