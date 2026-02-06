//! Screen modules for the TUI.

pub mod create;
pub mod home;
pub mod prompt;
pub mod view;

use crate::App;
use ratatui::{layout::Rect, Frame};

/// Trait for TUI screens.
pub trait Screen {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App);
}
