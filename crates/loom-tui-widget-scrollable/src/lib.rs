// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use loom_tui_theme::Theme;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::Style,
	widgets::StatefulWidget,
};

#[derive(Debug, Default, Clone)]
pub struct ScrollableState {
	pub offset: usize,
	pub content_height: usize,
	pub viewport_height: usize,
}

impl ScrollableState {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn scroll_up(&mut self, amount: usize) {
		self.offset = self.offset.saturating_sub(amount);
	}

	pub fn scroll_down(&mut self, amount: usize) {
		let max = self.max_offset();
		self.offset = (self.offset + amount).min(max);
	}

	pub fn scroll_to_top(&mut self) {
		self.offset = 0;
	}

	pub fn scroll_to_bottom(&mut self) {
		self.offset = self.max_offset();
	}

	pub fn set_content_height(&mut self, height: usize) {
		self.content_height = height;
		self.offset = self.offset.min(self.max_offset());
	}

	pub fn set_viewport_height(&mut self, height: usize) {
		self.viewport_height = height;
		self.offset = self.offset.min(self.max_offset());
	}

	pub fn max_offset(&self) -> usize {
		self.content_height.saturating_sub(self.viewport_height)
	}

	pub fn visible_range(&self) -> (usize, usize) {
		(self.offset, self.offset + self.viewport_height)
	}

	pub fn scrollbar_position(&self) -> f32 {
		let max = self.max_offset();
		if max == 0 {
			0.0
		} else {
			self.offset as f32 / max as f32
		}
	}
}

#[derive(Debug, Clone, Default)]
pub struct Scrollable {
	show_scrollbar: bool,
	track_style: Style,
	thumb_style: Style,
	theme: Option<Theme>,
	direction: TextDirection,
}



impl Scrollable {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn show_scrollbar(mut self, show: bool) -> Self {
		self.show_scrollbar = show;
		self
	}

	pub fn track_style(mut self, style: Style) -> Self {
		self.track_style = style;
		self
	}

	pub fn thumb_style(mut self, style: Style) -> Self {
		self.thumb_style = style;
		self
	}

	pub fn theme(mut self, theme: Theme) -> Self {
		self.theme = Some(theme);
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}
}

impl StatefulWidget for Scrollable {
	type State = ScrollableState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		state.viewport_height = area.height as usize;

		if !self.show_scrollbar || area.width == 0 || area.height == 0 {
			return;
		}

		if state.content_height <= state.viewport_height {
			return;
		}

		let is_rtl = self.direction.is_rtl();
		let scrollbar_x = if is_rtl {
			area.x
		} else {
			area.x + area.width - 1
		};
		let track_height = area.height as usize;

		if track_height == 0 {
			return;
		}

		let thumb_height = ((state.viewport_height as f32 / state.content_height as f32)
			* track_height as f32)
			.max(1.0) as usize;

		let thumb_offset = (state.scrollbar_position()
			* (track_height.saturating_sub(thumb_height)) as f32) as usize;

		let (track_style, thumb_style) = if let Some(ref theme) = self.theme {
			(
				Style::default().fg(theme.colors.text_muted),
				Style::default().fg(theme.colors.text),
			)
		} else {
			(self.track_style, self.thumb_style)
		};

		for y in 0..track_height {
			let cell_y = area.y + y as u16;
			let (symbol, style) = if y >= thumb_offset && y < thumb_offset + thumb_height {
				("█", thumb_style)
			} else {
				("░", track_style)
			};
			buf[(scrollbar_x, cell_y)].set_symbol(symbol).set_style(style);
		}
	}
}
