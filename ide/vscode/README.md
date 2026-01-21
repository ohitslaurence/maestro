<!--
Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
SPDX-License-Identifier: Proprietary
-->

# Loom AI Coding Assistant

Loom is an AI-powered coding assistant with agentic capabilities for Visual Studio Code.

## Features

- **Chat Interface**: Interactive chat panel for conversing with the AI assistant
- **Code Explanation**: Select code and ask Loom to explain it
- **Code Refactoring**: Select code and ask Loom to refactor it
- **Session Management**: Create new sessions, cancel ongoing operations, restart the agent

## Commands

| Command | Keybinding | Description |
|---------|------------|-------------|
| `Loom: Open Chat` | `Ctrl+Shift+L` / `Cmd+Shift+L` | Open the Loom chat panel |
| `Loom: New Session` | - | Start a new chat session |
| `Loom: Cancel Current Turn` | - | Cancel the current AI operation |
| `Loom: Restart Agent` | - | Restart the Loom agent |
| `Loom: Show Logs` | - | Show extension logs |
| `Loom: Explain Selection` | - | Explain the selected code |
| `Loom: Refactor Selection` | - | Refactor the selected code |

## Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `loom.loomPath` | string | `""` | Path to the Loom executable. If empty, uses PATH. |
| `loom.additionalArgs` | array | `[]` | Additional arguments to pass to the Loom agent. |
| `loom.logLevel` | string | `"info"` | Log level: error, warn, info, debug, trace |
| `loom.autoStart` | boolean | `true` | Automatically start the agent on activation. |
| `loom.serverUrl` | string | `""` | URL of the Loom server. If empty, starts a local agent. |

## Installation

### From VSIX

1. Download the `.vsix` file
2. Open VS Code
3. Run `Extensions: Install from VSIX...` from the Command Palette
4. Select the downloaded file

### From Source

```bash
cd ide/vscode
npm install
npm run compile
npm run package
```

## Development

```bash
# Install dependencies
npm install

# Watch for changes
npm run watch

# Run tests
npm test

# Lint
npm run lint

# Package extension
npm run package
```

## Requirements

- VS Code 1.85.0 or later
- Loom agent installed and available in PATH (or configured via `loom.loomPath`)

## License

Proprietary - See LICENSE file for details.
