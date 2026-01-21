<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Spool System

Version control for Loom, based on a fork of [jj (Jujutsu)](https://github.com/martinvonz/jj) with tapestry-themed naming.

## Overview

Spool is Loom's integrated version control system, providing a superior alternative to raw Git with first-class support for:
- Anonymous/work-in-progress changes (stitches)
- Automatic rebasing (rethreading)
- Conflict-aware operations (tangle handling)
- Operation log for full undo (unpick)
- Git compatibility (shuttle/draw)

## Terminology Mapping

| jj Term | Spool Term | Metaphor |
|---------|------------|----------|
| `.jj` directory | `.spool` | Where thread is wound/stored |
| Change | **Stitch** | Atomic unit of work |
| Change ID | **Stitch ID** | Unique stitch identifier |
| Commit | **Knot** | Tied-off stitch with message |
| Working copy | **Shuttle** | Active carrier moving through work |
| Revision | **Loop** | A completed pass |
| Conflict | **Tangle** / **Snag** | Threads crossing incorrectly |
| Bookmark | **Pin** | Marker on the thread |
| Rebase | **Rethread** | Moving stitches to new position |
| Squash | **Ply** | Twisting strands together |
| Split | **Unravel** | Separating into strands |
| Abandon | **Snip** | Cutting loose thread |
| Operation log | **Tension log** | Record of operations |
| Undo | **Unpick** | Removing stitches |
| Resolve | **Untangle** | Fixing a snag |

## CLI Commands

All commands are under `loom spool`:

| Command | jj Equivalent | Description |
|---------|---------------|-------------|
| `loom spool wind` | `jj init` | Initialize a new spool |
| `loom spool stitch` | `jj new` | Start a new stitch |
| `loom spool knot` | `jj commit` | Tie off with a message |
| `loom spool mark` | `jj describe` | Add/edit stitch description |
| `loom spool trace` | `jj log` | Show stitch history |
| `loom spool compare` | `jj diff` | Show differences |
| `loom spool tension` | `jj status` | Show current state |
| `loom spool rethread` | `jj rebase` | Move stitches |
| `loom spool ply` | `jj squash` | Combine stitches |
| `loom spool unravel` | `jj split` | Split a stitch |
| `loom spool snip` | `jj abandon` | Discard a stitch |
| `loom spool mend` | `jj restore` | Restore file contents |
| `loom spool pin` | `jj bookmark` | Manage pins |
| `loom spool shuttle` | `jj git push` | Push to remote |
| `loom spool draw` | `jj git fetch` | Fetch from remote |
| `loom spool tension-log` | `jj op log` | Show operation history |
| `loom spool unpick` | `jj undo` | Undo last operation |
| `loom spool untangle` | `jj resolve` | Resolve tangles |
| `loom spool show` | `jj show` | Show stitch details |
| `loom spool edit` | `jj edit` | Edit a stitch |
| `loom spool duplicate` | `jj duplicate` | Copy a stitch |

## Directory Structure

```
project/
├── .spool/                     # Spool metadata (replaces .jj)
│   ├── repo/
│   │   ├── store/              # Object store
│   │   │   ├── stitches/       # Change objects
│   │   │   ├── knots/          # Commit objects
│   │   │   └── trees/          # Tree objects
│   │   ├── tension-log/        # Operation log
│   │   └── index/              # Indexes
│   └── shuttle/                # Working copy state
└── .git/                       # Git backend (colocated)
```

## Crate Architecture

```
crates/
├── loom-common-spool/          # Core library (jj fork)
│   ├── lib.rs                  # Public API
│   ├── error.rs                # SpoolError types
│   ├── types.rs                # Stitch, StitchId, Pin, etc.
│   ├── repo.rs                 # SpoolRepo operations
│   ├── stitch.rs               # Stitch/change logic
│   ├── knot.rs                 # Commit operations
│   ├── shuttle.rs              # Working copy management
│   ├── rethread.rs             # Rebase logic
│   ├── ply.rs                  # Squash logic
│   ├── unravel.rs              # Split logic
│   ├── pin.rs                  # Bookmark/pin management
│   ├── tangle.rs               # Conflict types and resolution
│   ├── tension_log.rs          # Operation log
│   ├── revset.rs               # Revision set language
│   └── backend/
│       ├── mod.rs
│       ├── git.rs              # Git backend integration
│       └── store.rs            # Object store abstraction
│
├── loom-cli-spool/             # CLI implementation
│   ├── lib.rs
│   ├── formatter.rs            # Output formatting
│   ├── template.rs             # Log templates
│   └── commands/
│       ├── mod.rs
│       ├── wind.rs             # init
│       ├── stitch.rs           # new
│       ├── knot.rs             # commit
│       ├── mark.rs             # describe
│       ├── trace.rs            # log
│       ├── compare.rs          # diff
│       ├── tension.rs          # status
│       ├── rethread.rs         # rebase
│       ├── ply.rs              # squash
│       ├── unravel.rs          # split
│       ├── snip.rs             # abandon
│       ├── mend.rs             # restore
│       ├── pin.rs              # bookmark
│       ├── shuttle.rs          # git push
│       ├── draw.rs             # git fetch
│       ├── tension_log.rs      # op log
│       ├── unpick.rs           # undo
│       ├── untangle.rs         # resolve
│       ├── show.rs             # show
│       ├── edit.rs             # edit
│       └── duplicate.rs        # duplicate
│
└── loom-server-spool/          # Server-side (future)
    └── ...                     # Remote spool hosting
```

## Core Types

```rust
/// Unique identifier for a stitch (change)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StitchId(pub [u8; 16]);

/// A stitch represents an atomic unit of work
pub struct Stitch {
    pub id: StitchId,
    pub parents: Vec<StitchId>,
    pub tree_id: TreeId,
    pub description: String,
    pub author: Signature,
    pub committer: Signature,
    pub is_knotted: bool,  // true if committed
}

/// A pin marks a named point in history (like a bookmark/branch)
pub struct Pin {
    pub name: String,
    pub target: StitchId,
    pub is_tracking: bool,  // tracks a remote pin
}

/// Represents a tangle (conflict) in a file
pub struct Tangle {
    pub path: PathBuf,
    pub sides: Vec<TangleSide>,
}

pub enum TangleSide {
    Base(Vec<u8>),
    Left(Vec<u8>),
    Right(Vec<u8>),
}

/// The shuttle manages the working copy
pub struct Shuttle {
    pub stitch_id: StitchId,
    pub tree_state: TreeState,
}

/// Entry in the tension log (operation log)
pub struct TensionEntry {
    pub id: OperationId,
    pub parents: Vec<OperationId>,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub snapshot_before: RepoState,
    pub snapshot_after: RepoState,
}
```

## SpoolRepo API

```rust
pub struct SpoolRepo {
    store: Box<dyn Backend>,
    shuttle: Shuttle,
    tension_log: TensionLog,
}

impl SpoolRepo {
    /// Initialize a new spool in the given directory
    pub fn wind(path: &Path, colocate_git: bool) -> Result<Self>;
    
    /// Open an existing spool
    pub fn open(path: &Path) -> Result<Self>;
    
    /// Create a new stitch on top of the current one
    pub fn stitch(&mut self) -> Result<StitchId>;
    
    /// Tie off current stitch with a message
    pub fn knot(&mut self, message: &str) -> Result<()>;
    
    /// Set description on a stitch
    pub fn mark(&mut self, id: &StitchId, message: &str) -> Result<()>;
    
    /// Get stitch log
    pub fn trace(&self, revset: &str) -> Result<Vec<Stitch>>;
    
    /// Rethread (rebase) stitches
    pub fn rethread(&mut self, source: &StitchId, dest: &StitchId) -> Result<()>;
    
    /// Ply (squash) stitches together
    pub fn ply(&mut self, source: &StitchId, dest: &StitchId) -> Result<()>;
    
    /// Snip (abandon) a stitch
    pub fn snip(&mut self, id: &StitchId) -> Result<()>;
    
    /// Get current tangles
    pub fn tangles(&self) -> Result<Vec<Tangle>>;
    
    /// Untangle (resolve) a conflict
    pub fn untangle(&mut self, path: &Path, resolution: &[u8]) -> Result<()>;
    
    /// Unpick (undo) last operation
    pub fn unpick(&mut self) -> Result<()>;
    
    /// Shuttle (push) to remote
    pub fn shuttle(&mut self, remote: &str, pins: &[String]) -> Result<()>;
    
    /// Draw (fetch) from remote
    pub fn draw(&mut self, remote: &str) -> Result<()>;
}
```

## Git Interoperability

Spool maintains full Git compatibility:

- **Colocated mode**: `.spool` alongside `.git`, both updated together
- **Import**: Existing Git repos can be wound into spool
- **Export**: Stitches become Git commits when shuttled
- **Pins → Branches**: Pins map to Git branches

```bash
# Initialize with Git colocated
loom spool wind --git

# Work with existing Git repo
cd existing-git-repo
loom spool wind --git-repo .

# Push pins as branches
loom spool shuttle origin cannon
```

## Revset Language

Spool uses a revset query language (inherited from jj):

```bash
# All ancestors of current stitch
loom spool trace '::@'

# All stitches by author
loom spool trace 'author(ghuntley)'

# Stitches touching a file
loom spool trace 'file(src/main.rs)'

# Unpinned stitches (not on any pin)
loom spool trace 'heads(all()) ~ pins()'
```

## Integration with Loom Agent

The Loom agent uses spool for:

1. **Auto-stitching**: Each tool execution creates a new stitch
2. **Rollback**: Agent can unpick failed operations
3. **Branching**: Create pins for different approaches
4. **Context**: Stitch history provides conversation context

```rust
// In loom-core, after tool execution
if tool_modifies_files(&tool) {
    spool_repo.stitch()?;
    spool_repo.knot(&format!("Agent: {}", tool.name()))?;
}
```

## Configuration

In `loom.toml`:

```toml
[spool]
# Default to colocated Git
colocate_git = true

# Auto-stitch on file changes
auto_stitch = true

# Tension log retention (days)
tension_log_retention = 90

[spool.user]
name = "Geoffrey Huntley"
email = "ghuntley@ghuntley.com"
```

## Implementation Plan

### Phase 1: Core Library
1. Fork jj-lib, rename to loom-common-spool
2. Rename types: Change→Stitch, Bookmark→Pin, etc.
3. Rename directories: `.jj`→`.spool`
4. Update internal strings and error messages

### Phase 2: CLI
1. Create loom-cli-spool with command structure
2. Map jj commands to spool commands
3. Integrate into loom-cli main

### Phase 3: Agent Integration
1. Auto-stitch on tool execution
2. Unpick for rollback
3. Trace for context

### Phase 4: Server (Future)
1. Remote spool hosting
2. Collaborative stitching
3. Pin sync

## Dependencies

- `gix` - Git operations (gitoxide)
- `loom-common-config` - Configuration
- `loom-common-secret` - Credential handling
- `thiserror` - Error types
- `chrono` - Timestamps
- `serde` - Serialization

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum SpoolError {
    #[error("not a spool repository")]
    NotASpoolRepo,
    
    #[error("stitch not found: {0}")]
    StitchNotFound(StitchId),
    
    #[error("tangle in {path}: must untangle before continuing")]
    Tangled { path: PathBuf },
    
    #[error("pin already exists: {0}")]
    PinExists(String),
    
    #[error("nothing to unpick")]
    NothingToUnpick,
    
    #[error("git error: {0}")]
    Git(#[from] gix::Error),
    
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SpoolError>;
```
