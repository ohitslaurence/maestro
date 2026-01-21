// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn create_main_layout(area: Rect) -> Layout {
	let header_height = if area.height >= 20 { 3 } else { 1 };
	Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(header_height),
			Constraint::Min(1),
			Constraint::Length(1),
		])
}

pub fn create_content_layout(area: Rect) -> Layout {
	if area.width < 80 {
		Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
	} else {
		Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
	}
}
