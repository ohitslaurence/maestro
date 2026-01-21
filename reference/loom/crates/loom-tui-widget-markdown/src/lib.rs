// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use loom_tui_core::TextDirection;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
	buffer::Buffer,
	layout::{Alignment, Rect},
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::{Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Debug, Default, Clone)]
pub struct MarkdownState {
	pub scroll_offset: usize,
	cached_hash: u64,
	cached_lines: Option<Vec<Line<'static>>>,
}

impl MarkdownState {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn scroll_up(&mut self, amount: usize) {
		self.scroll_offset = self.scroll_offset.saturating_sub(amount);
	}

	pub fn scroll_down(&mut self, amount: usize) {
		self.scroll_offset = self.scroll_offset.saturating_add(amount);
	}

	fn get_or_compute_lines(
		&mut self,
		content: &str,
		_style: Style,
		direction: TextDirection,
		compute: impl FnOnce() -> Vec<Line<'static>>,
	) -> Vec<Line<'static>> {
		let mut hasher = DefaultHasher::new();
		content.hash(&mut hasher);
		direction.is_rtl().hash(&mut hasher);
		let hash = hasher.finish();

		if self.cached_hash == hash {
			if let Some(ref lines) = self.cached_lines {
				return lines.clone();
			}
		}

		let lines = compute();
		self.cached_hash = hash;
		self.cached_lines = Some(lines.clone());
		lines
	}
}

#[derive(Debug, Clone)]
pub struct Markdown {
	content: String,
	style: Style,
	direction: TextDirection,
}

impl Markdown {
	pub fn new(content: impl Into<String>) -> Self {
		Self {
			content: content.into(),
			style: Style::default(),
			direction: TextDirection::default(),
		}
	}

	pub fn style(mut self, style: Style) -> Self {
		self.style = style;
		self
	}

	pub fn direction(mut self, direction: TextDirection) -> Self {
		self.direction = direction;
		self
	}

	fn parse_to_lines(&self) -> Vec<Line<'static>> {
		let mut lines: Vec<Line<'static>> = Vec::new();
		let mut current_spans: Vec<Span<'static>> = Vec::new();
		let mut style_stack: Vec<Style> = vec![self.style];
		let mut list_stack: Vec<Option<u64>> = Vec::new();
		let mut link_stack: Vec<String> = Vec::new();
		let mut in_code_block = false;
		let mut code_block_content = String::new();
		let mut code_block_lang: Option<String> = None;
		let mut in_block_quote = false;

		let mut current_row_cells: Vec<String> = Vec::new();
		let mut table_rows: Vec<Vec<String>> = Vec::new();
		let mut in_table_cell = false;
		let mut current_cell_text = String::new();

		let is_rtl = self.direction.is_rtl();

		let options = Options::all();
		let parser = Parser::new_ext(&self.content, options);

		for event in parser {
			match event {
				Event::Start(tag) => match tag {
					Tag::Heading { level, .. } => {
						let header_style = self.style.add_modifier(Modifier::BOLD);
						style_stack.push(header_style);
						let prefix = "#".repeat(level as usize) + " ";
						current_spans.push(Span::styled(prefix, header_style));
					}
					Tag::Paragraph => {
						if in_block_quote {
							if is_rtl {
								current_spans.push(Span::styled(" <", self.style));
							} else {
								current_spans.push(Span::styled("> ", self.style));
							}
						}
					}
					Tag::CodeBlock(kind) => {
						in_code_block = true;
						code_block_content.clear();
						code_block_lang = match kind {
							CodeBlockKind::Fenced(lang) => {
								let lang_str = lang.to_string();
								if lang_str.is_empty() {
									None
								} else {
									Some(lang_str)
								}
							}
							CodeBlockKind::Indented => None,
						};
					}
					Tag::List(start) => {
						list_stack.push(start);
					}
					Tag::Item => {
						let depth = list_stack.len().saturating_sub(1);
						let indent = "  ".repeat(depth);

						if let Some(list_type) = list_stack.last_mut() {
							match list_type {
								Some(num) => {
									if is_rtl {
										current_spans.push(Span::styled(
											format!(" .{}{}", num, indent),
											self.style,
										));
									} else {
										current_spans.push(Span::styled(
											format!("{}{}. ", indent, num),
											self.style,
										));
									}
									*num += 1;
								}
								None => {
									if is_rtl {
										current_spans
											.push(Span::styled(format!(" •{}", indent), self.style));
									} else {
										current_spans
											.push(Span::styled(format!("{}• ", indent), self.style));
									}
								}
							}
						}
					}
					Tag::Emphasis => {
						let italic_style = style_stack
							.last()
							.copied()
							.unwrap_or(self.style)
							.add_modifier(Modifier::ITALIC);
						style_stack.push(italic_style);
					}
					Tag::Strong => {
						let bold_style = style_stack
							.last()
							.copied()
							.unwrap_or(self.style)
							.add_modifier(Modifier::BOLD);
						style_stack.push(bold_style);
					}
					Tag::Link { dest_url, .. } => {
						let underline_style = style_stack
							.last()
							.copied()
							.unwrap_or(self.style)
							.add_modifier(Modifier::UNDERLINED);
						style_stack.push(underline_style);
						link_stack.push(dest_url.to_string());
					}
					Tag::BlockQuote(_) => {
						in_block_quote = true;
						let italic_style = style_stack
							.last()
							.copied()
							.unwrap_or(self.style)
							.add_modifier(Modifier::ITALIC);
						style_stack.push(italic_style);
					}
					Tag::Table(_) => {
						table_rows.clear();
					}
					Tag::TableHead => {
						current_row_cells.clear();
					}
					Tag::TableRow => {
						current_row_cells.clear();
					}
					Tag::TableCell => {
						in_table_cell = true;
						current_cell_text.clear();
					}
					_ => {}
				},
				Event::End(tag_end) => match tag_end {
					TagEnd::Heading(_) => {
						style_stack.pop();
						if !current_spans.is_empty() {
							lines.push(Line::from(std::mem::take(&mut current_spans)));
						}
						lines.push(Line::from(""));
					}
					TagEnd::Paragraph => {
						if !current_spans.is_empty() {
							lines.push(Line::from(std::mem::take(&mut current_spans)));
						}
						lines.push(Line::from(""));
					}
					TagEnd::CodeBlock => {
						in_code_block = false;
						let code_style = self.style.add_modifier(Modifier::DIM);
						if let Some(ref lang) = code_block_lang {
							lines.push(Line::from(Span::styled(
								format!("  [{}]", lang),
								code_style,
							)));
						}
						for line in code_block_content.lines() {
							lines.push(Line::from(Span::styled(
								format!("  {}", line),
								code_style,
							)));
						}
						lines.push(Line::from(""));
						code_block_content.clear();
						code_block_lang = None;
					}
					TagEnd::List(_) => {
						list_stack.pop();
						if list_stack.is_empty() {
							lines.push(Line::from(""));
						}
					}
					TagEnd::Item => {
						if !current_spans.is_empty() {
							lines.push(Line::from(std::mem::take(&mut current_spans)));
						}
					}
					TagEnd::Emphasis | TagEnd::Strong => {
						style_stack.pop();
					}
					TagEnd::Link => {
						style_stack.pop();
						if let Some(url) = link_stack.pop() {
							let dim_style = self.style.add_modifier(Modifier::DIM);
							if is_rtl {
								current_spans.push(Span::styled(format!("({}) ", url), dim_style));
							} else {
								current_spans.push(Span::styled(format!(" ({})", url), dim_style));
							}
						}
					}
					TagEnd::BlockQuote(_) => {
						in_block_quote = false;
						style_stack.pop();
						if !current_spans.is_empty() {
							lines.push(Line::from(std::mem::take(&mut current_spans)));
						}
						lines.push(Line::from(""));
					}
					TagEnd::Table => {
						for (i, row) in table_rows.iter().enumerate() {
							let row_text = row.join(" | ");
							lines.push(Line::from(Span::styled(
								format!("| {} |", row_text),
								self.style,
							)));
							if i == 0 {
								let separator = row.iter().map(|c| "-".repeat(c.len())).collect::<Vec<_>>().join(" | ");
								lines.push(Line::from(Span::styled(
									format!("| {} |", separator),
									self.style,
								)));
							}
						}
						table_rows.clear();
						lines.push(Line::from(""));
					}
					TagEnd::TableHead => {
						if !current_row_cells.is_empty() {
							table_rows.push(std::mem::take(&mut current_row_cells));
						}
					}
					TagEnd::TableRow => {
						if !current_row_cells.is_empty() {
							table_rows.push(std::mem::take(&mut current_row_cells));
						}
					}
					TagEnd::TableCell => {
						in_table_cell = false;
						current_row_cells.push(std::mem::take(&mut current_cell_text));
					}
					_ => {}
				},
				Event::Text(text) => {
					if in_code_block {
						code_block_content.push_str(&text);
					} else if in_table_cell {
						current_cell_text.push_str(&text);
					} else {
						let current_style = style_stack.last().copied().unwrap_or(self.style);
						current_spans.push(Span::styled(text.to_string(), current_style));
					}
				}
				Event::Code(code) => {
					if in_table_cell {
						current_cell_text.push_str(&format!("`{}`", code));
					} else {
						let code_style = self.style.add_modifier(Modifier::DIM);
						current_spans.push(Span::styled(format!("`{}`", code), code_style));
					}
				}
				Event::SoftBreak => {
					if in_table_cell {
						current_cell_text.push(' ');
					} else {
						let current_style = style_stack.last().copied().unwrap_or(self.style);
						current_spans.push(Span::styled(" ", current_style));
					}
				}
				Event::HardBreak => {
					if !current_spans.is_empty() {
						lines.push(Line::from(std::mem::take(&mut current_spans)));
					}
				}
				_ => {}
			}
		}

		if !current_spans.is_empty() {
			lines.push(Line::from(current_spans));
		}

		lines
	}
}

impl StatefulWidget for Markdown {
	type State = MarkdownState;

	fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
		let content = self.content.clone();
		let style = self.style;
		let direction = self.direction;
		let lines = state.get_or_compute_lines(&content, style, direction, || self.parse_to_lines());

		let alignment = if direction.is_rtl() {
			Alignment::Right
		} else {
			Alignment::Left
		};

		let paragraph = Paragraph::new(lines)
			.style(style)
			.wrap(Wrap { trim: false })
			.alignment(alignment)
			.scroll((state.scroll_offset as u16, 0));
		paragraph.render(area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_markdown_new() {
		let md = Markdown::new("# Hello");
		assert_eq!(md.content, "# Hello");
	}

	#[test]
	fn test_markdown_state_default() {
		let state = MarkdownState::default();
		assert_eq!(state.scroll_offset, 0);
	}

	#[test]
	fn test_parse_header() {
		let md = Markdown::new("# Header");
		let lines = md.parse_to_lines();
		assert!(!lines.is_empty());
	}

	#[test]
	fn test_parse_bullet_list() {
		let md = Markdown::new("- item 1\n- item 2");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans
				.iter()
				.any(|s| s.content.contains("•") || s.content.contains("item"))
		}));
	}

	#[test]
	fn test_parse_numbered_list() {
		let md = Markdown::new("1. first\n2. second");
		let lines = md.parse_to_lines();
		assert!(lines
			.iter()
			.any(|l| { l.spans.iter().any(|s| s.content.contains("1.")) }));
	}

	#[test]
	fn test_parse_code_block() {
		let md = Markdown::new("```\ncode here\n```");
		let lines = md.parse_to_lines();
		assert!(!lines.is_empty());
	}

	#[test]
	fn test_parse_inline_code() {
		let md = Markdown::new("Use `code` here");
		let lines = md.parse_to_lines();
		assert!(lines
			.iter()
			.any(|l| { l.spans.iter().any(|s| s.content.contains("`code`")) }));
	}

	#[test]
	fn test_parse_nested_list() {
		let md = Markdown::new("- level 1\n  - level 2\n    - level 3");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains("  "))
		}));
	}

	#[test]
	fn test_parse_link() {
		let md = Markdown::new("[text](https://example.com)");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains("example.com"))
		}));
	}

	#[test]
	fn test_parse_block_quote() {
		let md = Markdown::new("> quoted text");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains(">"))
		}));
	}

	#[test]
	fn test_parse_table() {
		let md = Markdown::new("| A | B |\n|---|---|\n| 1 | 2 |");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains("|"))
		}));
	}

	#[test]
	fn test_parse_fenced_code_block_with_lang() {
		let md = Markdown::new("```rust\nfn main() {}\n```");
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains("rust"))
		}));
	}

	#[test]
	fn test_caching() {
		let mut state = MarkdownState::new();
		let md = Markdown::new("# Test");
		let content = md.content.clone();
		let style = md.style;
		let direction = md.direction;

		let lines1 = state.get_or_compute_lines(&content, style, direction, || md.parse_to_lines());
		assert!(state.cached_lines.is_some());

		let hash_before = state.cached_hash;
		let md2 = Markdown::new("# Test");
		let lines2 = state.get_or_compute_lines(&content, style, direction, || md2.parse_to_lines());
		assert_eq!(hash_before, state.cached_hash);
		assert_eq!(lines1.len(), lines2.len());
	}

	#[test]
	fn test_direction_builder() {
		let md = Markdown::new("# Hello").direction(TextDirection::Rtl);
		assert!(md.direction.is_rtl());
	}

	#[test]
	fn test_rtl_bullet_list() {
		let md = Markdown::new("- item").direction(TextDirection::Rtl);
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains(" •"))
		}));
	}

	#[test]
	fn test_rtl_block_quote() {
		let md = Markdown::new("> quoted").direction(TextDirection::Rtl);
		let lines = md.parse_to_lines();
		assert!(lines.iter().any(|l| {
			l.spans.iter().any(|s| s.content.contains("<"))
		}));
	}
}
