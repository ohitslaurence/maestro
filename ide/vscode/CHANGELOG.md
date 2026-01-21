<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Changelog

All notable changes to the Loom VS Code extension will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release of the Loom VS Code extension
- Chat view panel with streaming message support
- Session management (create, switch, persist across restarts)
- Agent process management with automatic restart on crash
- Commands: Open Chat, New Session, Cancel Current Turn, Restart Agent, Show Logs
- Context commands: Explain Selection, Refactor Selection
- Configuration options: loomPath, additionalArgs, logLevel, autoStart, serverUrl
- Keyboard shortcut: Ctrl+Shift+L (Cmd+Shift+L on macOS) to open chat
- Editor context menu integration for selection commands

### Dependencies

- @agentclientprotocol/sdk ^0.12.0

## [0.1.0] - 2025-01-01

### Added

- Initial development release
- Core ACP client integration
- Basic chat functionality
- Tool call visualization

[Unreleased]: https://github.com/ghuntley/loom/compare/vscode-v0.1.0...HEAD
[0.1.0]: https://github.com/ghuntley/loom/releases/tag/vscode-v0.1.0
