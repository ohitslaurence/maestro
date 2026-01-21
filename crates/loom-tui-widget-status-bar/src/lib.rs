// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use loom_tui_theme::Theme;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Style, Stylize},
	text::{Line, Span},
	widgets::Widget,
};

#[derive(Debug, Clone)]
pub struct StatusItem {
	pub label: String,
	pub value: String,
}

#[derive(Debug, Clone)]
pub struct StatusBar {
	items: Vec<StatusItem>,
	shortcuts: Vec<(String, String)>,
	style: Style,
	direction: TextDirection,
}

impl Default for StatusBar {
	fn default() -> Self {
		Self {
			items: Vec::new(),
			shortcuts: Vec::new(),
			style: Style::default(),
			direction: TextDirection::Ltr,
		}
	}
}

impl StatusBar {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn item(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
		self.items.push(StatusItem {
			label: label.into(),
			value: value.into(),
		});
		self
	}

	pub fn shortcut(mut self, key: impl Into<String>, desc: impl Into<String>) -> Self {
		self.shortcuts.push((key.into(), desc.into()));
		self
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}
}

impl Widget for StatusBar {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let theme = Theme::default();
		let is_rtl = self.direction.is_rtl();

		if self.style != Style::default() {
			buf.set_style(area, self.style);
		}

		let mut shortcut_spans = Vec::new();
		for (i, (key, desc)) in self.shortcuts.iter().enumerate() {
			if i > 0 {
				shortcut_spans.push(Span::raw(" | "));
			}
			shortcut_spans.push(Span::raw(key).bold().fg(theme.colors.accent));
			shortcut_spans.push(Span::raw(" "));
			shortcut_spans.push(Span::raw(desc));
		}
		let shortcut_line = Line::from(shortcut_spans);
		let shortcut_width = shortcut_line.width() as u16;

		let available_for_items = area.width.saturating_sub(shortcut_width + 1);

		let mut item_spans = Vec::new();
		let mut item_total_width = 0usize;
		for (i, item) in self.items.iter().enumerate() {
			let separator = if i > 0 { " | " } else { "" };
			let item_str = format!("{}{}: {}", separator, item.label, item.value);
			let item_width = item_str.len();

			if item_total_width + item_width > available_for_items as usize {
				let remaining = available_for_items as usize - item_total_width;
				if remaining > 3 {
					let truncated: String = item_str.chars().take(remaining.saturating_sub(1)).collect();
					if i > 0 {
						item_spans.push(Span::raw(" | "));
					}
					item_spans.push(Span::raw(format!("{}…", truncated.trim_start_matches(" | "))));
				}
				break;
			}

			if i > 0 {
				item_spans.push(Span::raw(" | "));
			}
			item_spans.push(Span::raw(&item.label).bold());
			item_spans.push(Span::raw(": "));
			item_spans.push(Span::raw(&item.value));
			item_total_width += item_width;
		}

		let item_line = Line::from(item_spans);
		let item_width = item_line.width() as u16;

		if is_rtl {
			buf.set_line(area.x, area.y, &shortcut_line, shortcut_width);

			let items_x = area.right().saturating_sub(item_width);
			if items_x > area.x {
				buf.set_line(items_x, area.y, &item_line, item_width);
			}
		} else {
			buf.set_line(area.x, area.y, &item_line, available_for_items);

			let shortcut_x = area.right().saturating_sub(shortcut_width);
			if shortcut_x > area.x {
				buf.set_line(shortcut_x, area.y, &shortcut_line, shortcut_width);
			}
		}
	}
}
