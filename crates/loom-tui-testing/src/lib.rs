// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::{backend::TestBackend, layout::Rect, Frame, Terminal};

#[derive(Debug, Clone, Default)]
pub struct Theme;

pub trait InteractiveComponent {
	fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme);
	fn handle_event(&mut self, event: Event);
}

pub struct TestHarness {
	terminal: Terminal<TestBackend>,
	theme: Theme,
}

impl TestHarness {
	pub fn new(width: u16, height: u16) -> Self {
		let backend = TestBackend::new(width, height);
		let terminal = Terminal::new(backend).expect("failed to create terminal");
		Self {
			terminal,
			theme: Theme,
		}
	}

	pub fn with_theme(mut self, theme: Theme) -> Self {
		self.theme = theme;
		self
	}

	pub fn theme(&self) -> &Theme {
		&self.theme
	}

	pub fn render<F>(&mut self, render_fn: F) -> &TestBackend
	where
		F: FnOnce(&mut Frame, Rect, &Theme),
	{
		let theme = &self.theme;
		self.terminal
			.draw(|frame| {
				let area = frame.area();
				render_fn(frame, area, theme);
			})
			.expect("failed to draw");
		self.terminal.backend()
	}

	pub fn assert_snapshot<F>(&mut self, name: &str, render_fn: F)
	where
		F: FnOnce(&mut Frame, Rect, &Theme),
	{
		let backend = self.render(render_fn);
		let output = buffer_to_string(backend);
		insta::assert_snapshot!(name, output);
	}

	pub fn buffer_lines(&self) -> Vec<String> {
		let buffer = self.terminal.backend().buffer();
		let area = buffer.area;
		let mut lines = Vec::new();

		for y in area.y..area.y + area.height {
			let mut line = String::new();
			for x in area.x..area.x + area.width {
				let cell = &buffer[(x, y)];
				line.push_str(cell.symbol());
			}
			lines.push(line);
		}

		lines
	}

	pub fn find_text(&self, needle: &str) -> Option<(usize, usize)> {
		let lines = self.buffer_lines();
		for (row, line) in lines.iter().enumerate() {
			if let Some(col) = line.find(needle) {
				return Some((row, col));
			}
		}
		None
	}
}

pub struct ComponentHarness<C> {
	pub harness: TestHarness,
	pub component: C,
}

impl<C: InteractiveComponent> ComponentHarness<C> {
	pub fn new(component: C, width: u16, height: u16) -> Self {
		Self {
			harness: TestHarness::new(width, height),
			component,
		}
	}

	pub fn render(&mut self) -> &TestBackend {
		let component = &self.component;
		let theme = self.harness.theme.clone();
		self.harness.render(|frame, area, _| {
			component.render(frame, area, &theme);
		})
	}

	pub fn send_event(&mut self, event: Event) -> &TestBackend {
		self.component.handle_event(event);
		self.render()
	}

	pub fn send_key(&mut self, key: KeyEvent) -> &TestBackend {
		self.send_event(Event::Key(key))
	}

	pub fn send_mouse(&mut self, mouse: MouseEvent) -> &TestBackend {
		self.send_event(Event::Mouse(mouse))
	}

	pub fn assert_snapshot(&mut self, name: &str) {
		let component = &self.component;
		let theme = self.harness.theme.clone();
		self.harness.assert_snapshot(name, |frame, area, _| {
			component.render(frame, area, &theme);
		});
	}

	pub fn assert_state_sequence<T, F>(
		&mut self,
		events: &[Event],
		mut extract_state: F,
		expected: &[T],
	) where
		T: PartialEq + std::fmt::Debug + Clone,
		F: FnMut(&C) -> T,
	{
		assert_eq!(
			events.len(),
			expected.len(),
			"events and expected states must have the same length"
		);

		for (i, (event, expected_state)) in events.iter().zip(expected.iter()).enumerate() {
			self.component.handle_event(event.clone());
			let actual_state = extract_state(&self.component);
			assert_eq!(
				&actual_state, expected_state,
				"state mismatch at step {}: expected {:?}, got {:?}",
				i, expected_state, actual_state
			);
		}
	}

	pub fn user_press_enter(&mut self) -> &TestBackend {
		self.send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
	}

	pub fn user_type(&mut self, text: &str) -> &TestBackend {
		for ch in text.chars() {
			self.component
				.handle_event(Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)));
		}
		self.render()
	}
}

fn buffer_to_string(backend: &TestBackend) -> String {
	let buffer = backend.buffer();
	let area = buffer.area;
	let mut output = String::new();

	for y in area.y..area.y + area.height {
		for x in area.x..area.x + area.width {
			let cell = &buffer[(x, y)];
			output.push_str(cell.symbol());
		}
		if y < area.y + area.height - 1 {
			output.push('\n');
		}
	}

	output
}

#[cfg(feature = "proptest")]
pub mod strategies {
	use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
	use proptest::prelude::*;

	pub fn key_event_strategy() -> impl Strategy<Value = KeyEvent> {
		let key_code = prop_oneof![
			Just(KeyCode::Enter),
			Just(KeyCode::Esc),
			Just(KeyCode::Backspace),
			Just(KeyCode::Tab),
			Just(KeyCode::Up),
			Just(KeyCode::Down),
			Just(KeyCode::Left),
			Just(KeyCode::Right),
			Just(KeyCode::Home),
			Just(KeyCode::End),
			Just(KeyCode::PageUp),
			Just(KeyCode::PageDown),
			Just(KeyCode::Delete),
			proptest::char::range('a', 'z').prop_map(KeyCode::Char),
			proptest::char::range('A', 'Z').prop_map(KeyCode::Char),
			proptest::char::range('0', '9').prop_map(KeyCode::Char),
			proptest::sample::select(vec![' ', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')'])
				.prop_map(KeyCode::Char),
		];

		let modifiers = prop_oneof![
			Just(KeyModifiers::NONE),
			Just(KeyModifiers::SHIFT),
			Just(KeyModifiers::CONTROL),
			Just(KeyModifiers::ALT),
		];

		(key_code, modifiers).prop_map(|(code, mods)| KeyEvent::new(code, mods))
	}

	pub fn event_sequence_strategy(max_len: usize) -> impl Strategy<Value = Vec<Event>> {
		proptest::collection::vec(key_event_strategy().prop_map(Event::Key), 0..=max_len)
	}
}

#[cfg(feature = "proptest")]
pub use strategies::{event_sequence_strategy, key_event_strategy};

#[cfg(test)]
mod tests {
	use super::*;
	use ratatui::widgets::{Block, Borders};

	#[test]
	fn test_harness_creation() {
		let harness = TestHarness::new(80, 24);
		assert_eq!(harness.terminal.backend().buffer().area.width, 80);
		assert_eq!(harness.terminal.backend().buffer().area.height, 24);
	}

	#[test]
	fn test_render() {
		let mut harness = TestHarness::new(20, 5);
		harness.render(|frame, area, _theme| {
			let block = Block::default().borders(Borders::ALL).title("Test");
			frame.render_widget(block, area);
		});
		let output = buffer_to_string(harness.terminal.backend());
		assert!(output.contains("Test"));
	}

	#[test]
	fn test_buffer_lines() {
		let mut harness = TestHarness::new(20, 5);
		harness.render(|frame, area, _theme| {
			let block = Block::default().borders(Borders::ALL).title("Hello");
			frame.render_widget(block, area);
		});
		let lines = harness.buffer_lines();
		assert_eq!(lines.len(), 5);
		assert!(lines[0].contains("Hello"));
	}

	#[test]
	fn test_find_text() {
		let mut harness = TestHarness::new(20, 5);
		harness.render(|frame, area, _theme| {
			let block = Block::default().borders(Borders::ALL).title("FindMe");
			frame.render_widget(block, area);
		});
		let pos = harness.find_text("FindMe");
		assert!(pos.is_some());
		let (row, _col) = pos.unwrap();
		assert_eq!(row, 0);
	}

	struct TestComponent {
		counter: i32,
	}

	impl InteractiveComponent for TestComponent {
		fn render(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
			let block = Block::default()
				.borders(Borders::ALL)
				.title(format!("Count: {}", self.counter));
			frame.render_widget(block, area);
		}

		fn handle_event(&mut self, event: Event) {
			if let Event::Key(key) = event {
				match key.code {
					KeyCode::Up => self.counter += 1,
					KeyCode::Down => self.counter -= 1,
					_ => {}
				}
			}
		}
	}

	#[test]
	fn test_component_harness() {
		let component = TestComponent { counter: 0 };
		let mut harness = ComponentHarness::new(component, 20, 5);

		harness.render();
		assert!(harness.harness.find_text("Count: 0").is_some());

		harness.send_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
		assert!(harness.harness.find_text("Count: 1").is_some());
	}

	#[test]
	fn test_user_type() {
		struct TextInput {
			text: String,
		}

		impl InteractiveComponent for TextInput {
			fn render(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
				let block = Block::default()
					.borders(Borders::ALL)
					.title(self.text.clone());
				frame.render_widget(block, area);
			}

			fn handle_event(&mut self, event: Event) {
				if let Event::Key(key) = event {
					if let KeyCode::Char(c) = key.code {
						self.text.push(c);
					}
				}
			}
		}

		let component = TextInput {
			text: String::new(),
		};
		let mut harness = ComponentHarness::new(component, 20, 5);

		harness.user_type("hi");
		assert_eq!(harness.component.text, "hi");
	}

	#[test]
	fn test_state_sequence() {
		let component = TestComponent { counter: 0 };
		let mut harness = ComponentHarness::new(component, 20, 5);

		let events = vec![
			Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
			Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
			Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
		];

		harness.assert_state_sequence(&events, |c| c.counter, &[1, 2, 1]);
	}
}
