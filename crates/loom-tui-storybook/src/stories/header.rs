// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use loom_tui_widget_header::Header;
use ratatui::{layout::Rect, widgets::Widget, Frame};

struct SimpleHeader;

impl StoryComponent for SimpleHeader {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let header = Header::new("Loom TUI");
		header.render(area, frame.buffer_mut());
	}
}

struct HeaderWithSubtitle;

impl StoryComponent for HeaderWithSubtitle {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let header = Header::new("Loom TUI").subtitle("v0.1.0");
		header.render(area, frame.buffer_mut());
	}
}

pub fn header_story() -> Story {
	Story::new("Header", "Title header with optional subtitle")
		.variant("Simple", SimpleHeader)
		.variant("With Subtitle", HeaderWithSubtitle)
}
