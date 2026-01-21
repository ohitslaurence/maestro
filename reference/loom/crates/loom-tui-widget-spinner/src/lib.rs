// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! A spinner widget for ratatui TUI applications.
//!
//! # Example
//!
//! ```no_run
//! use std::time::Duration;
//! use loom_tui_widget_spinner::{Spinner, SpinnerKind, SpinnerState};
//!
//! let mut state = SpinnerState::default();
//! let spinner = Spinner::from_label("Loading...")
//!     .kind(SpinnerKind::Dots);
//!
//! // In your event loop, call state.tick() at regular intervals.
//! // Recommended tick rate: 80ms for Dots/Dot, 120ms for Line.
//! state.tick();
//! ```

use ratatui::prelude::*;
use ratatui::widgets::StatefulWidget;

const DOTS_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const LINE_FRAMES: &[&str] = &["-", "\\", "|", "/"];
const DOT_FRAMES: &[&str] = &["⠁", "⠂", "⠄", "⠂"];

#[derive(Clone, Copy, Debug, Default)]
pub enum SpinnerKind {
    #[default]
    Dots,
    Line,
    Dot,
}

impl SpinnerKind {
    pub fn frames(&self) -> &'static [&'static str] {
        match self {
            SpinnerKind::Dots => DOTS_FRAMES,
            SpinnerKind::Line => LINE_FRAMES,
            SpinnerKind::Dot => DOT_FRAMES,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SpinnerState {
    frame: usize,
}

impl SpinnerState {
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }
}

#[derive(Clone, Debug)]
pub struct Spinner {
    label: Option<String>,
    text_style: Style,
    kind: SpinnerKind,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            label: None,
            text_style: Style::default(),
            kind: SpinnerKind::default(),
        }
    }

    pub fn from_label(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            text_style: Style::default(),
            kind: SpinnerKind::default(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn text_style(mut self, style: Style) -> Self {
        self.text_style = style;
        self
    }

    pub fn kind(mut self, kind: SpinnerKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for Spinner {
    type State = SpinnerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frames = self.kind.frames();
        let frame_char = frames[state.frame % frames.len()];

        let text = match &self.label {
            Some(label) => format!("{} {}", frame_char, label),
            None => frame_char.to_string(),
        };

        let x = area.x;
        let y = area.y;

        buf.set_string(x, y, &text, self.text_style);
    }
}
