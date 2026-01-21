<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# TUI System Specification

Terminal User Interface (TUI) for Loom using [Ratatui](https://ratatui.rs/) v0.30.0.

## Overview

A modular, component-based TUI architecture where each widget is a separate crate for maximum
cargo2nix cache efficiency. Includes visual snapshot testing via `insta` and Storybook-style
component development with hot reload support.

## Architecture

### Crate Structure

Each widget is its own crate to maximize incremental compilation speed with cargo2nix:

```
crates/
├── loom-tui-core/                    # Core traits, actions, events
├── loom-tui-component/               # Component trait + registry
├── loom-tui-theme/                   # Theming (colors, borders, styles)
├── loom-tui-testing/                 # Test harness, insta snapshot helpers
├── loom-tui-storybook/               # Component gallery binary
├── loom-tui-hot/                     # Subsecond hot-reload integration
├── loom-tui-app/                     # Main TUI application binary
│
├── loom-tui-widget-message-list/     # Chat message display
├── loom-tui-widget-input-box/        # Text input with editing
├── loom-tui-widget-tool-panel/       # Tool execution display
├── loom-tui-widget-thread-list/      # Thread browser sidebar
├── loom-tui-widget-status-bar/       # Bottom status bar
├── loom-tui-widget-header/           # Top header/title bar
├── loom-tui-widget-markdown/         # Markdown rendering
├── loom-tui-widget-scrollable/       # Scrollable container
├── loom-tui-widget-spinner/          # Loading spinner
└── loom-tui-widget-modal/            # Modal dialog
```

### Dependency Graph

```
loom-tui-app
├── loom-tui-component
│   └── loom-tui-core
├── loom-tui-theme
├── loom-tui-widget-message-list
│   ├── loom-tui-component
│   ├── loom-tui-core
│   └── loom-tui-theme
├── loom-tui-widget-input-box
│   └── ...
└── loom-tui-widget-*
    └── ...

loom-tui-storybook
├── loom-tui-component
├── loom-tui-widget-*
└── loom-tui-hot (optional)

loom-tui-testing
├── loom-tui-core
├── loom-tui-theme
└── insta
```

## Core Crates

### loom-tui-core

Core types and event handling infrastructure.

```rust
// Action enum for all TUI actions
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tick,
    Quit,
    Render,
    Resize(u16, u16),
    FocusNext,
    FocusPrev,
    ScrollUp(usize),
    ScrollDown(usize),
    Submit,
    Cancel,
    Custom { kind: &'static str, payload: String },
}

// Event types
pub enum Event {
    Key(crossterm::event::KeyEvent),
    Mouse(crossterm::event::MouseEvent),
    Resize(u16, u16),
    Tick,
}

// Event source abstraction
#[async_trait]
pub trait EventSource: Send {
    async fn next(&mut self) -> Option<Event>;
}

// Render context passed to all components
pub struct RenderContext<'a> {
    pub theme: &'a Theme,
    pub focus: FocusState,
}
```

### loom-tui-component

The core Component trait following ratatui's component architecture pattern.

```rust
use async_trait::async_trait;
use loom_tui_core::{Action, Event, RenderContext};
use ratatui::layout::Rect;
use ratatui::Frame;

/// Core component trait - each TUI component implements this
#[async_trait]
pub trait Component: Send + Sync {
    /// Unique identifier for focus management
    fn id(&self) -> &str;

    /// Initialize component (called once on mount)
    fn init(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }

    /// Handle input events, return resulting actions
    fn handle_event(&mut self, event: &Event) -> Vec<Action>;

    /// Process actions (from self or parent), return follow-up actions
    fn update(&mut self, action: &Action) -> Vec<Action>;

    /// Render the component to the given area
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext);

    /// Whether this component can receive focus
    fn focusable(&self) -> bool {
        true
    }
}

/// For components with external state (like List selection)
pub trait StatefulComponent: Send + Sync {
    type State: Default;

    fn id(&self) -> &str;
    fn handle_event(&mut self, event: &Event, state: &mut Self::State) -> Vec<Action>;
    fn update(&mut self, action: &Action, state: &mut Self::State) -> Vec<Action>;
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext, state: &mut Self::State);
}
```

### loom-tui-theme

Centralized theming system.

```rust
use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub name: String,
    pub colors: ColorPalette,
    pub borders: BorderStyle,
    pub text: TextStyles,
}

pub struct ColorPalette {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub muted: Color,
    pub selection: Color,
}

pub struct TextStyles {
    pub normal: Style,
    pub bold: Style,
    pub dim: Style,
    pub italic: Style,
    pub code: Style,
    pub link: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self { /* ... */ }
    pub fn light() -> Self { /* ... */ }
}
```

### loom-tui-testing

Test harness for visual snapshot testing with `insta`.

```rust
use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal, Frame};
use ratatui::layout::Rect;
use loom_tui_theme::Theme;

/// Test harness for widget snapshot testing
pub struct TestHarness {
    terminal: Terminal<TestBackend>,
    theme: Theme,
}

impl TestHarness {
    pub fn new(width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).unwrap();
        Self {
            terminal,
            theme: Theme::default(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render a component and return the backend for snapshot assertion
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
            .unwrap();
        self.terminal.backend()
    }

    /// Render and assert snapshot
    pub fn assert_snapshot<F>(&mut self, name: &str, render_fn: F)
    where
        F: FnOnce(&mut Frame, Rect, &Theme),
    {
        let backend = self.render(render_fn);
        assert_snapshot!(name, format!("{}", backend));
    }
}

/// Macro for concise widget snapshot tests
#[macro_export]
macro_rules! widget_snapshot {
    ($name:ident, $width:expr, $height:expr, $widget:expr) => {
        #[test]
        fn $name() {
            let mut harness = $crate::TestHarness::new($width, $height);
            harness.assert_snapshot(stringify!($name), |frame, area, _theme| {
                frame.render_widget($widget, area);
            });
        }
    };
}
```

## Widget Crate Structure

Each widget crate follows this structure:

```
loom-tui-widget-spinner/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, re-exports
│   ├── widget.rs           # Widget/Component implementation
│   ├── state.rs            # State struct (for StatefulWidget)
│   └── stories.rs          # Storybook story definitions
└── tests/
    ├── render_test.rs      # Snapshot tests
    └── snapshots/          # insta snapshot files (auto-generated)
        ├── render_test__spinner_default.snap
        └── render_test__spinner_with_label.snap
```

### Example: Spinner Widget

```rust
// crates/loom-tui-widget-spinner/src/widget.rs

use ratatui::{
    widgets::StatefulWidget,
    buffer::Buffer,
    layout::Rect,
    style::Style,
};

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Default)]
pub struct SpinnerState {
    pub frame: usize,
}

impl SpinnerState {
    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % FRAMES.len();
    }
}

#[derive(Debug, Clone)]
pub struct Spinner {
    label: Option<String>,
    style: Style,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            label: None,
            style: Style::default(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl StatefulWidget for Spinner {
    type State = SpinnerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let frame = FRAMES[state.frame % FRAMES.len()];
        let text = match &self.label {
            Some(label) => format!("{} {}", frame, label),
            None => frame.to_string(),
        };
        buf.set_string(area.x, area.y, &text, self.style);
    }
}
```

### Snapshot Test Example

```rust
// crates/loom-tui-widget-spinner/tests/render_test.rs

use loom_tui_testing::TestHarness;
use loom_tui_widget_spinner::{Spinner, SpinnerState};
use ratatui::widgets::StatefulWidget;

#[test]
fn test_spinner_default() {
    let mut harness = TestHarness::new(20, 1);
    let mut state = SpinnerState::default();

    harness.assert_snapshot("spinner_default", |frame, area, _| {
        Spinner::new().render(area, frame.buffer_mut(), &mut state);
    });
}

#[test]
fn test_spinner_with_label() {
    let mut harness = TestHarness::new(30, 1);
    let mut state = SpinnerState::default();

    harness.assert_snapshot("spinner_with_label", |frame, area, _| {
        Spinner::new()
            .label("Loading...")
            .render(area, frame.buffer_mut(), &mut state);
    });
}
```

## Storybook

Component gallery for isolated development and visual testing.

### Story Definition

```rust
// crates/loom-tui-storybook/src/lib.rs

pub struct Story {
    pub name: &'static str,
    pub description: &'static str,
    pub variants: Vec<StoryVariant>,
}

pub struct StoryVariant {
    pub name: &'static str,
    pub render: Box<dyn Fn(&mut Frame, Rect)>,
}
```

### Storybook Features

- Left panel: List of components + variants
- Right panel: Live component preview
- Bottom panel: Component props/state inspector
- Hot reload: See changes instantly (via subsecond)
- Keyboard navigation: j/k to navigate, Enter to select, Esc to go back

## Hot Reload

### Option 1: Dioxus Subsecond (Fastest, ~130ms patches)

[Subsecond](https://docs.rs/subsecond) provides true Rust hot-patching by intercepting the
linker and patching symbols at runtime.

```rust
// crates/loom-tui-hot/src/lib.rs

use subsecond;

/// Run the TUI with hot-reload support
pub fn run_hot<F>(mut tick: F) -> Result<()>
where
    F: FnMut(&mut App) -> Action + 'static,
{
    loop {
        subsecond::call(|| {
            tick(&mut app);
        });
    }
}
```

Requires adding `#[hot]` macro or wrapping tick function in `subsecond::call()`.

### Option 2: bacon (Simpler, full rebuild)

```toml
# bacon.toml
[jobs.storybook]
command = ["cargo", "run", "-p", "loom-tui-storybook"]
watch = ["crates/loom-tui-*"]

[jobs.test-widgets]
command = ["cargo", "test", "-p", "loom-tui-widget-*"]
watch = ["crates/loom-tui-widget-*"]
```

## Visual Snapshot Testing Workflow

```bash
# Run all widget snapshot tests
cargo test -p 'loom-tui-widget-*'

# Review snapshot changes interactively
cargo insta review

# Update all snapshots
cargo insta accept

# Run specific widget tests
cargo test -p loom-tui-widget-spinner
```

Snapshots are stored at:
```
crates/loom-tui-widget-spinner/tests/snapshots/
├── render_test__spinner_default.snap
├── render_test__spinner_with_label.snap
└── render_test__spinner_frame_0.snap
```

## cargo2nix Integration

Each TUI crate is individually cacheable via cargo2nix:

```nix
# Add to flake.nix packages section
loom-tui-core-c2n = (rustPkgs.workspace.loom-tui-core {});
loom-tui-component-c2n = (rustPkgs.workspace.loom-tui-component {});
loom-tui-theme-c2n = (rustPkgs.workspace.loom-tui-theme {});
loom-tui-testing-c2n = (rustPkgs.workspace.loom-tui-testing {});
loom-tui-storybook-c2n = (rustPkgs.workspace.loom-tui-storybook {});
loom-tui-app-c2n = (rustPkgs.workspace.loom-tui-app {});

# Widget crates
loom-tui-widget-message-list-c2n = (rustPkgs.workspace.loom-tui-widget-message-list {});
loom-tui-widget-input-box-c2n = (rustPkgs.workspace.loom-tui-widget-input-box {});
loom-tui-widget-tool-panel-c2n = (rustPkgs.workspace.loom-tui-widget-tool-panel {});
loom-tui-widget-thread-list-c2n = (rustPkgs.workspace.loom-tui-widget-thread-list {});
loom-tui-widget-status-bar-c2n = (rustPkgs.workspace.loom-tui-widget-status-bar {});
loom-tui-widget-header-c2n = (rustPkgs.workspace.loom-tui-widget-header {});
loom-tui-widget-markdown-c2n = (rustPkgs.workspace.loom-tui-widget-markdown {});
loom-tui-widget-spinner-c2n = (rustPkgs.workspace.loom-tui-widget-spinner {});
loom-tui-widget-modal-c2n = (rustPkgs.workspace.loom-tui-widget-modal {});
loom-tui-widget-scrollable-c2n = (rustPkgs.workspace.loom-tui-widget-scrollable {});
```

## Dependencies

```toml
# workspace Cargo.toml additions
[workspace.dependencies]
ratatui = "0.30"
crossterm = "0.29"
insta = { version = "1.42", features = ["yaml"] }
subsecond = { version = "0.7", optional = true }
```

## Widgets

### Core Widgets

| Widget | Purpose |
|--------|---------|
| `message-list` | Display chat messages with user/assistant roles |
| `input-box` | Text input with cursor, selection, history |
| `tool-panel` | Show tool execution status and results |
| `thread-list` | Browse and select conversation threads |
| `status-bar` | Bottom bar with status info, shortcuts |
| `header` | Top bar with title, thread info |
| `markdown` | Render markdown content (code blocks, lists) |
| `scrollable` | Generic scrollable container |
| `spinner` | Animated loading indicator |
| `modal` | Modal dialog overlay |

### Widget Implementation Guidelines

1. **Implement `Widget` or `StatefulWidget`** trait from ratatui
2. **Implement `Component`** trait for event handling
3. **Provide stories** for Storybook visualization
4. **Write snapshot tests** for visual regression testing
5. **Use theme colors** from `RenderContext` for consistent styling

## Development Workflow

```bash
# 1. Start storybook for component development
cargo run -p loom-tui-storybook

# 2. With hot reload (using bacon)
bacon storybook

# 3. Run tests with snapshot review
cargo test -p loom-tui-widget-spinner
cargo insta review

# 4. Build all TUI crates
nix build .#loom-tui-app-c2n
```

## References

- [Ratatui Documentation](https://ratatui.rs/)
- [Ratatui Component Template](https://github.com/ratatui/templates/tree/main/component)
- [insta Snapshot Testing](https://insta.rs/)
- [Dioxus Subsecond](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload/)
