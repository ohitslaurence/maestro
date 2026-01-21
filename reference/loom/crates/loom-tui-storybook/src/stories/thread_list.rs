// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::{Story, StoryComponent};
use crossterm::event::{KeyCode, KeyEvent};
use loom_tui_widget_thread_list::{ThreadItem, ThreadList, ThreadListState};
use ratatui::{layout::Rect, widgets::StatefulWidget, Frame};

struct EmptyThreadList {
	state: ThreadListState,
}

impl StoryComponent for EmptyThreadList {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let list = ThreadList::new(vec![]);
		list.render(area, frame.buffer_mut(), &mut self.state);
	}
}

struct ThreadListWithThreads {
	threads: Vec<ThreadItem>,
	state: ThreadListState,
}

impl ThreadListWithThreads {
	fn new() -> Self {
		let threads = vec![
			ThreadItem::new("1", "Rust Error Handling")
				.preview("How do I handle errors in Rust?")
				.timestamp("10:30")
				.unread(true),
			ThreadItem::new("2", "Building a CLI App")
				.preview("I want to create a command line tool...")
				.timestamp("Yesterday"),
			ThreadItem::new("3", "Async Programming")
				.preview("Can you explain async/await?")
				.timestamp("Dec 28"),
		];
		Self {
			threads,
			state: ThreadListState::default(),
		}
	}
}

impl StoryComponent for ThreadListWithThreads {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		let list = ThreadList::new(self.threads.clone());
		list.render(area, frame.buffer_mut(), &mut self.state);
	}

	fn handle_key(&mut self, key: KeyEvent) {
		match key.code {
			KeyCode::Up => self.state.select_prev(),
			KeyCode::Down => self.state.select_next(self.threads.len()),
			_ => {}
		}
	}
}

pub fn thread_list_story() -> Story {
	Story::new("ThreadList", "List of conversation threads")
		.variant("Empty", EmptyThreadList { state: ThreadListState::default() })
		.variant("With Threads", ThreadListWithThreads::new())
}
