// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use loom_tui_widget_status_bar::StatusBar;
use ratatui::{layout::Rect, widgets::Widget, Frame};

struct BasicStatusBar;

impl StoryComponent for BasicStatusBar {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let status = StatusBar::new()
			.item("Model", "claude-3.5-sonnet")
			.item("Tokens", "1234");
		status.render(area, frame.buffer_mut());
	}
}

struct StatusBarWithShortcuts;

impl StoryComponent for StatusBarWithShortcuts {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let status = StatusBar::new()
			.item("Model", "claude-3.5-sonnet")
			.shortcut("q", "quit")
			.shortcut("?", "help")
			.shortcut("n", "new");
		status.render(area, frame.buffer_mut());
	}
}

pub fn status_bar_story() -> Story {
	Story::new("StatusBar", "Status bar with items and shortcuts")
		.variant("Basic", BasicStatusBar)
		.variant("With Shortcuts", StatusBarWithShortcuts)
}
