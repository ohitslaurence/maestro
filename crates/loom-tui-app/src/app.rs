// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
	layout::{Constraint, Direction, Layout, Rect},
	widgets::{Block, Borders},
	Frame,
};

use loom_tui_core::LocaleContext;
use loom_tui_theme::{LayoutDirection, Theme};
use loom_tui_widget_header::Header;
use loom_tui_widget_input_box::{InputBox, InputBoxState};
use loom_tui_widget_message_list::{Message, MessageList, MessageListState, MessageRole};
use loom_tui_widget_spinner::{Spinner, SpinnerState};
use loom_tui_widget_status_bar::StatusBar;
use loom_tui_widget_thread_list::{ThreadItem, ThreadList, ThreadListState};

use crate::layout::{create_content_layout, create_main_layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
	Input,
	MessageList,
	ThreadList,
}

pub struct AppState {
	pub messages: Vec<Message>,
	pub input_state: InputBoxState,
	pub message_list_state: MessageListState,
	pub spinner_state: SpinnerState,
	pub thread_list_state: ThreadListState,
	pub threads: Vec<ThreadItem>,
	pub active_thread_id: Option<String>,
	pub is_loading: bool,
	pub should_quit: bool,
	pub focus: Focus,
	pub locale: LocaleContext,
}

impl Default for AppState {
	fn default() -> Self {
		Self {
			messages: vec![
				Message::new(MessageRole::System, "Welcome to Loom! Type a message to get started."),
			],
			input_state: InputBoxState::new(),
			message_list_state: MessageListState::default(),
			spinner_state: SpinnerState::default(),
			thread_list_state: ThreadListState::default(),
			threads: vec![
				ThreadItem::new("1", "New Thread")
					.preview("Start a new conversation...")
					.timestamp("now")
					.unread(true),
			],
			active_thread_id: None,
			is_loading: false,
			should_quit: false,
			focus: Focus::Input,
			locale: LocaleContext::default(),
		}
	}
}

pub struct App {
	pub state: AppState,
	pub theme: Theme,
}

impl App {
	pub fn new() -> Self {
		Self {
			state: AppState::default(),
			theme: Theme::dark(),
		}
	}

	pub fn handle_key_event(&mut self, key: KeyEvent) {
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
			self.state.should_quit = true;
			return;
		}

		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
			let new_locale = match self.state.locale.locale.as_str() {
				"en" => "ar",
				"ar" => "he",
				"he" => "en",
				_ => "en",
			};
			self.state.locale = LocaleContext::new(new_locale);
			return;
		}

		match key.code {
			KeyCode::Char('q') if self.state.focus != Focus::Input => {
				self.state.should_quit = true;
			}
			KeyCode::Tab => {
				self.state.focus = match self.state.focus {
					Focus::Input => Focus::MessageList,
					Focus::MessageList => Focus::ThreadList,
					Focus::ThreadList => Focus::Input,
				};
			}
			KeyCode::Up => match self.state.focus {
				Focus::MessageList => {
					self.state.message_list_state.scroll_up(1);
				}
				Focus::ThreadList => {
					self.state.thread_list_state.select_prev();
				}
				_ => {}
			},
			KeyCode::Down => match self.state.focus {
				Focus::MessageList => {
					let total = self.state.messages.len();
					self.state.message_list_state.scroll_down(1, total);
				}
				Focus::ThreadList => {
					let total = self.state.threads.len();
					self.state.thread_list_state.select_next(total);
				}
				_ => {}
			},
			KeyCode::Enter => match self.state.focus {
				Focus::Input => {
					let content = self.state.input_state.content().to_string();
					if !content.is_empty() {
						self.state.messages.push(Message::new(MessageRole::User, content));
						self.state.input_state.clear();
						let total = self.state.messages.len();
						self.state.message_list_state.scroll_to_bottom(total);
					}
				}
				Focus::ThreadList => {
					if let Some(selected) = self.state.thread_list_state.selected() {
						if let Some(thread) = self.state.threads.get(selected) {
							self.state.active_thread_id = Some(thread.id.clone());
						}
					}
				}
				_ => {}
			},
			KeyCode::Char(c) if self.state.focus == Focus::Input => {
				self.state.input_state.insert_char(c);
			}
			KeyCode::Backspace if self.state.focus == Focus::Input => {
				self.state.input_state.delete_char();
			}
			KeyCode::Left if self.state.focus == Focus::Input => {
				self.state.input_state.move_cursor_left();
			}
			KeyCode::Right if self.state.focus == Focus::Input => {
				self.state.input_state.move_cursor_right();
			}
			KeyCode::Home if self.state.focus == Focus::Input => {
				self.state.input_state.move_cursor_start();
			}
			KeyCode::End if self.state.focus == Focus::Input => {
				self.state.input_state.move_cursor_end();
			}
			_ => {}
		}
	}

	pub fn tick(&mut self) {
		if self.state.is_loading {
			self.state.spinner_state.tick();
		}
	}

	pub fn render(&mut self, frame: &mut Frame) {
		let main_layout = create_main_layout(frame.area());
		let main_areas = main_layout.split(frame.area());

		let header_area = main_areas[0];
		let content_area = main_areas[1];
		let status_area = main_areas[2];

		let compact_header = header_area.height < 3;
		if compact_header {
			let header = Header::new("Loom").style(self.theme.text.bold);
			frame.render_widget(header, header_area);
		} else {
			let header = Header::new("Loom").style(self.theme.text.bold);
			let header_block = Block::default()
				.borders(Borders::BOTTOM)
				.border_style(self.theme.borders.normal);
			frame.render_widget(header_block, header_area);
			let header_inner = Rect {
				x: header_area.x + 1,
				y: header_area.y + 1,
				width: header_area.width.saturating_sub(2),
				height: 1,
			};
			frame.render_widget(header, header_inner);
		}

		let direction = self.state.locale.direction;
		let layout_dir = LayoutDirection::new(direction);

		let (sidebar_area, chat_area) = if content_area.width < 80 {
			let content_layout = create_content_layout(content_area);
			let content_areas = content_layout.split(content_area);
			(content_areas[0], content_areas[1])
		} else {
			let sidebar_width = content_area.width / 4;
			layout_dir.split_horizontal(content_area, sidebar_width)
		};

		let sidebar_focused = self.state.focus == Focus::ThreadList;
		let sidebar_border_style = if sidebar_focused {
			self.theme.borders.focused
		} else {
			self.theme.borders.normal
		};
		let sidebar_block = Block::default()
			.title("Threads")
			.borders(Borders::ALL)
			.border_style(sidebar_border_style);
		let sidebar_inner = sidebar_block.inner(sidebar_area);
		frame.render_widget(sidebar_block, sidebar_area);

		let thread_list = ThreadList::new(self.state.threads.clone())
			.style(self.theme.text.normal)
			.focused(sidebar_focused);
		frame.render_stateful_widget(
			thread_list,
			sidebar_inner,
			&mut self.state.thread_list_state,
		);

		let chat_layout = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Min(1), Constraint::Length(3)])
			.split(chat_area);

		let messages_area = chat_layout[0];
		let input_area = chat_layout[1];

		let messages_focused = self.state.focus == Focus::MessageList;
		let messages_border_style = if messages_focused {
			self.theme.borders.focused
		} else {
			self.theme.borders.normal
		};
		let messages_block = Block::default()
			.title("Messages")
			.borders(Borders::ALL)
			.border_style(messages_border_style);
		let messages_inner = messages_block.inner(messages_area);
		frame.render_widget(messages_block, messages_area);

		let message_list =
			MessageList::new(self.state.messages.clone()).style(self.theme.text.normal);
		frame.render_stateful_widget(
			message_list,
			messages_inner,
			&mut self.state.message_list_state,
		);

		if self.state.is_loading {
			let spinner = Spinner::new()
				.label("Thinking...")
				.text_style(self.theme.text.normal);
			let spinner_area = Rect {
				x: messages_inner.x,
				y: messages_inner.bottom().saturating_sub(1),
				width: messages_inner.width,
				height: 1,
			};
			frame.render_stateful_widget(spinner, spinner_area, &mut self.state.spinner_state);
		}

		let input_focused = self.state.focus == Focus::Input;
		let input_border_style = if input_focused {
			self.theme.borders.focused
		} else {
			self.theme.borders.normal
		};
		let input_block = Block::default()
			.title("Input")
			.borders(Borders::ALL)
			.border_style(input_border_style);
		let input_inner = input_block.inner(input_area);
		frame.render_widget(input_block, input_area);

		let input_box = InputBox::new()
			.placeholder("Type a message...")
			.style(self.theme.text.normal)
			.focused(input_focused);
		frame.render_stateful_widget(input_box, input_inner, &mut self.state.input_state);

		let status_bar = self.build_status_bar();
		frame.render_widget(status_bar, status_area);
	}

	fn build_status_bar(&self) -> StatusBar {
		let locale_display = format!("[{}]", self.state.locale.locale.to_uppercase());
		let mut status = StatusBar::new()
			.item("Focus", match self.state.focus {
				Focus::Input => "Input",
				Focus::MessageList => "Messages",
				Focus::ThreadList => "Threads",
			})
			.item("Locale", &locale_display);

		match self.state.focus {
			Focus::Input => {
				status = status
					.shortcut("Enter", "Send")
					.shortcut("Tab", "Switch")
					.shortcut("Ctrl+L", "Locale")
					.shortcut("Ctrl+C", "Quit");
			}
			Focus::MessageList => {
				status = status
					.shortcut("↑↓", "Scroll")
					.shortcut("Tab", "Switch")
					.shortcut("Ctrl+L", "Locale")
					.shortcut("q", "Quit");
			}
			Focus::ThreadList => {
				status = status
					.shortcut("↑↓", "Select")
					.shortcut("Enter", "Open")
					.shortcut("Tab", "Switch")
					.shortcut("Ctrl+L", "Locale")
					.shortcut("q", "Quit");
			}
		}

		status
	}

	pub fn should_quit(&self) -> bool {
		self.state.should_quit
	}
}

impl Default for App {
	fn default() -> Self {
		Self::new()
	}
}
