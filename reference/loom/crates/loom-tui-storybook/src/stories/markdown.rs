// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_widget_markdown::{Markdown, MarkdownState};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct HeadersMarkdown {
	state: MarkdownState,
}

impl StoryComponent for HeadersMarkdown {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let content = r#"# Heading 1

## Heading 2

### Heading 3

Regular paragraph text goes here."#;
		let md = Markdown::new(content);
		md.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Up => self.state.scroll_up(1),
			KeyCode::Down => self.state.scroll_down(1),
			_ => {}
		}
	}
}

struct CodeBlockMarkdown {
	state: MarkdownState,
}

impl StoryComponent for CodeBlockMarkdown {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let content = r#"Here is some code:

```rust
fn main() {
    println!("Hello, world!");
}
```

And some `inline code` too."#;
		let md = Markdown::new(content);
		md.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Up => self.state.scroll_up(1),
			KeyCode::Down => self.state.scroll_down(1),
			_ => {}
		}
	}
}

struct ListsMarkdown {
	state: MarkdownState,
}

impl StoryComponent for ListsMarkdown {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let content = r#"## Bullet List

- First item
- Second item
- Third item

## Numbered List

1. Step one
2. Step two
3. Step three"#;
		let md = Markdown::new(content);
		md.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Up => self.state.scroll_up(1),
			KeyCode::Down => self.state.scroll_down(1),
			_ => {}
		}
	}
}

pub fn markdown_story() -> Story {
	Story::new("Markdown", "Rendered markdown content")
		.variant("Headers", HeadersMarkdown { state: MarkdownState::default() })
		.variant("Code Block", CodeBlockMarkdown { state: MarkdownState::default() })
		.variant("Lists", ListsMarkdown { state: MarkdownState::default() })
}
