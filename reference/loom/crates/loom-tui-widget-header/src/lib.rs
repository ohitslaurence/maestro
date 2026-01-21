// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_tui_core::TextDirection;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::Style,
	text::{Line, Span},
	widgets::Widget,
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct Header {
	title: String,
	subtitle: Option<String>,
	icon: Option<String>,
	status: Option<String>,
	style: Style,
	direction: TextDirection,
}

impl Header {
	pub fn new(title: impl Into<String>) -> Self {
		Self {
			title: title.into(),
			subtitle: None,
			icon: None,
			status: None,
			style: Style::default(),
			direction: TextDirection::Ltr,
		}
	}

	pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
		self.subtitle = Some(subtitle.into());
		self
	}

	pub fn icon(mut self, icon: impl Into<String>) -> Self {
		self.icon = Some(icon.into());
		self
	}

	pub fn status(mut self, status: impl Into<String>) -> Self {
		self.status = Some(status.into());
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

impl Widget for Header {
	fn render(self, area: Rect, buf: &mut Buffer) {
		if area.height == 0 || area.width == 0 {
			return;
		}

		buf.set_style(area, self.style);

		let is_rtl = self.direction.is_rtl();

		let icon_width = self.icon.as_ref().map(|i| UnicodeWidthStr::width(i.as_str()) + 1).unwrap_or(0);
		let status_width = self.status.as_ref().map(|s| UnicodeWidthStr::width(s.as_str()) + 1).unwrap_or(0);
		let subtitle_width = self.subtitle.as_ref().map(|s| UnicodeWidthStr::width(s.as_str()) + 1).unwrap_or(0);
		let title_width = UnicodeWidthStr::width(self.title.as_str());

		let available_width = area.width as usize;

		let (left_reserved, right_reserved) = if is_rtl {
			(status_width + subtitle_width, icon_width)
		} else {
			(icon_width, status_width + subtitle_width)
		};

		let center_start = left_reserved;
		let center_end = available_width.saturating_sub(right_reserved);
		let center_width = center_end.saturating_sub(center_start);

		if let Some(ref icon) = self.icon {
			let icon_x = if is_rtl {
				area.width.saturating_sub(icon_width as u16)
			} else {
				0
			};
			let icon_line = Line::from(Span::styled(icon, self.style));
			buf.set_line(area.x + icon_x, area.y, &icon_line, icon_width as u16);
		}

		let title_x = if title_width <= center_width {
			center_start + (center_width.saturating_sub(title_width)) / 2
		} else {
			center_start
		};
		let title_line = Line::from(Span::styled(&self.title, self.style));
		buf.set_line(area.x + title_x as u16, area.y, &title_line, center_width as u16);

		if let Some(ref status) = self.status {
			let actual_status_width = UnicodeWidthStr::width(status.as_str());
			let status_x = if is_rtl {
				0
			} else {
				area.width.saturating_sub(actual_status_width as u16)
			};
			let status_line = Line::from(Span::styled(status, self.style));
			buf.set_line(area.x + status_x, area.y, &status_line, actual_status_width as u16);
		}

		if let Some(ref subtitle) = self.subtitle {
			let actual_subtitle_width = UnicodeWidthStr::width(subtitle.as_str());
			let subtitle_x = if is_rtl {
				status_width as u16
			} else {
				area.width.saturating_sub(actual_subtitle_width as u16 + status_width as u16)
			};
			if subtitle_x > left_reserved as u16 {
				let subtitle_line = Line::from(Span::styled(subtitle, self.style));
				buf.set_line(area.x + subtitle_x, area.y, &subtitle_line, actual_subtitle_width as u16);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_header_new() {
		let header = Header::new("Test Title");
		assert_eq!(header.title, "Test Title");
		assert!(header.subtitle.is_none());
		assert!(header.icon.is_none());
		assert!(header.status.is_none());
		assert_eq!(header.direction, TextDirection::Ltr);
	}

	#[test]
	fn test_header_with_subtitle() {
		let header = Header::new("Title").subtitle("Subtitle");
		assert_eq!(header.title, "Title");
		assert_eq!(header.subtitle, Some("Subtitle".to_string()));
	}

	#[test]
	fn test_header_with_icon() {
		let header = Header::new("Title").icon("🔧");
		assert_eq!(header.icon, Some("🔧".to_string()));
	}

	#[test]
	fn test_header_with_status() {
		let header = Header::new("Title").status("Online");
		assert_eq!(header.status, Some("Online".to_string()));
	}

	#[test]
	fn test_header_full_builder() {
		let header = Header::new("Title")
			.icon("📁")
			.subtitle("Details")
			.status("Ready");

		assert_eq!(header.title, "Title");
		assert_eq!(header.icon, Some("📁".to_string()));
		assert_eq!(header.subtitle, Some("Details".to_string()));
		assert_eq!(header.status, Some("Ready".to_string()));
	}

	#[test]
	fn test_header_direction() {
		let header = Header::new("Title").direction(TextDirection::Rtl);
		assert_eq!(header.direction, TextDirection::Rtl);
		assert!(header.direction.is_rtl());
	}

	#[test]
	fn test_header_default_direction_is_ltr() {
		let header = Header::new("Title");
		assert_eq!(header.direction, TextDirection::Ltr);
		assert!(header.direction.is_ltr());
	}
}
