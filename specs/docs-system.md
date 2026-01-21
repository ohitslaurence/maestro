<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Documentation System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2026-01-03

---

## 1. Overview

### Purpose

The documentation system provides a `/docs` route in loom-web for product documentation, authored in MDX/Svelte markdown and organized using the [Diátaxis framework](https://diataxis.fr/). It features reusable components styled with the Threadwork design system, full-text search via Pagefind, and syntax highlighting via Shiki.

### Goals

- **Diátaxis Structure**: Organize docs into tutorials, how-to guides, reference, and explanation
- **MDX Authoring**: Write documentation in markdown with embedded Svelte components
- **Reusable Components**: Callouts, code blocks, tabs, steps, asciinema embeds
- **Full-text Search**: Static search via Pagefind (no external services)
- **Auto-navigation**: Sidebar, breadcrumbs, TOC, prev/next generated from file structure
- **Threadwork Styling**: Monospace typography, natural dye colors, thread motifs

### Non-Goals

- Internationalization (i18n) / multi-language support
- Versioned documentation (v1, v2, etc.)
- Headless CMS integration
- Real-time collaborative editing of docs

### Known Limitations

- **No offline search**: Search requires connectivity to loom-server

---

## 2. Architecture

### 2.1 Technology Stack

| Layer | Technology |
|-------|------------|
| Content | mdsvex (`.svx` files with frontmatter) |
| Markdown plugins | remark-gfm, rehype-slug, rehype-autolink-headings |
| Syntax highlighting | Shiki via rehype-pretty-code |
| Search | Pagefind (post-build static indexing) |
| Schema validation | Zod |
| Framework | SvelteKit 2, Svelte 5 (runes) |
| Styling | Tailwind 4, Threadwork design tokens |

### 2.2 Route Structure

```
src/routes/docs/
  +layout.svelte          # Docs chrome: sidebar, search, TOC
  +layout.ts              # Auto-nav builder, prev/next computation
  +page.svelte            # Redirects to /docs/tutorials
  
  tutorials/
    +page.svelte          # Category index
    getting-started/
      +page.svx           # Individual doc page
    first-thread/
      +page.svx
      
  how-to/
    +page.svelte
    configure-auth/
      +page.svx
    share-thread/
      +page.svx
      
  reference/
    +page.svelte
    api/
      +page.svx
    cli/
      +page.svx
    configuration/
      +page.svx
      
  explanation/
    +page.svelte
    architecture/
      +page.svx
    state-machines/
      +page.svx
```

### 2.3 URL Pattern

```
/docs                           → redirects to /docs/tutorials
/docs/tutorials                 → tutorials index
/docs/tutorials/getting-started → individual tutorial
/docs/how-to/share-thread       → how-to guide
/docs/reference/api             → API reference
/docs/explanation/architecture  → conceptual explanation
```

---

## 3. Content Schema

### 3.1 Frontmatter Schema (Zod)

```typescript
// src/lib/docs/schema.ts
import { z } from 'zod';

export const DiataxisCategory = z.enum([
  'tutorial',
  'how-to', 
  'reference',
  'explanation'
]);

export const DocSchema = z.object({
  title: z.string(),
  summary: z.string().optional(),
  diataxis: DiataxisCategory,
  order: z.number().default(100),
  tags: z.array(z.string()).optional(),
  draft: z.boolean().default(false),
  updatedAt: z.string().optional(),
});

export type DocMeta = z.infer<typeof DocSchema>;
export type DiataxisCategory = z.infer<typeof DiataxisCategory>;
```

### 3.2 Example Frontmatter

```yaml
---
title: Getting Started with Loom
summary: Create your first thread and learn the basics of AI-assisted coding.
diataxis: tutorial
order: 10
tags:
  - onboarding
  - beginner
---
```

### 3.3 Content Layer

```typescript
// src/lib/docs/content.ts
import { DocSchema, type DocMeta } from './schema';

export interface DocEntry {
  slug: string;
  path: string;           // URL path: /docs/tutorials/getting-started
  category: string;       // tutorials, how-to, reference, explanation
  meta: DocMeta;
  toc: TocItem[];         // Extracted headings
  component: SvelteComponent;
}

export interface TocItem {
  id: string;
  text: string;
  depth: number;          // 2 for h2, 3 for h3
}

export interface NavSection {
  category: string;
  title: string;          // "Tutorials", "How-to Guides", etc.
  items: NavItem[];
}

export interface NavItem {
  slug: string;
  path: string;
  title: string;
  order: number;
}
```

---

## 4. Components

### 4.1 Component Library

Located in `src/lib/docs/`:

| Component | Purpose |
|-----------|---------|
| `Callout.svelte` | Info/warning/danger/tip admonitions |
| `CodeBlock.svelte` | Syntax-highlighted code with copy button |
| `AsciinemaPlayer.svelte` | Embedded terminal recordings |
| `Tabs.svelte` + `TabItem.svelte` | Tabbed content (code groups) |
| `LinkCard.svelte` | Highlighted link cards |
| `Steps.svelte` + `Step.svelte` | Numbered step sequences |
| `TableOfContents.svelte` | Page-level heading navigation |
| `Breadcrumbs.svelte` | Hierarchical location display |
| `PrevNext.svelte` | Previous/next page navigation |
| `DocSearch.svelte` | Pagefind search interface |

### 4.2 Callout Variants

Uses Threadwork natural dye status colors:

| Variant | Color Token | Use Case |
|---------|-------------|----------|
| `info` | `--color-info` (Woad Blue) | General information |
| `tip` | `--color-success` (Lichen Green) | Best practices, tips |
| `warning` | `--color-warning` (Weld Gold) | Cautions, deprecations |
| `danger` | `--color-error` (Madder Red) | Breaking changes, critical warnings |

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte';
  
  interface Props {
    variant?: 'info' | 'tip' | 'warning' | 'danger';
    title?: string;
    children: Snippet;
  }
  
  let { variant = 'info', title, children }: Props = $props();
</script>
```

### 4.3 CodeBlock Features

- Shiki syntax highlighting (build-time)
- Line numbers (optional)
- Line highlighting via meta: `{1,3-5}`
- Diff highlighting: `+` and `-` prefixes
- Copy button with feedback
- Filename/title header
- Threadwork-themed syntax colors

### 4.4 AsciinemaPlayer

Iframe-based embed for SSR safety:

```svelte
<script lang="ts">
  interface Props {
    id: string;           // Asciinema cast ID
    rows?: number;
    cols?: number;
    autoplay?: boolean;
    loop?: boolean;
    speed?: number;
    theme?: string;
  }
</script>
```

### 4.5 Tabs / TabItem

For code groups and alternative content:

```svelte
<!-- Usage in MDX -->
<Tabs>
  <TabItem label="pnpm">
    ```bash
    pnpm add loom-cli
    ```
  </TabItem>
  <TabItem label="npm">
    ```bash
    npm install loom-cli
    ```
  </TabItem>
</Tabs>
```

### 4.6 Steps

Numbered procedural steps:

```svelte
<Steps>
  <Step title="Install the CLI">
    Download and install loom-cli for your platform.
  </Step>
  <Step title="Authenticate">
    Run `loom login` to connect your account.
  </Step>
  <Step title="Start weaving">
    Create your first thread with `loom new`.
  </Step>
</Steps>
```

---

## 5. Navigation

### 5.1 Sidebar

- Organized by Diátaxis category
- Collapsible sections
- Active page highlighting
- Generated from file structure via `import.meta.glob`

### 5.2 Table of Contents

- Extracted from h2/h3 headings at build time
- Sticky positioning on desktop
- Scroll spy for current section highlighting
- Click to smooth-scroll

### 5.3 Breadcrumbs

Format: `Docs / Tutorials / Getting Started`

### 5.4 Prev/Next

- Computed from sorted docs in same category
- Shows title and category label
- Thread-styled link cards

---

## 6. Search

### 6.1 Architecture

Search uses server-side SQLite FTS5 instead of client-side Pagefind to work around SPA mode limitations:

```
┌─────────────────┐    build    ┌─────────────────┐
│   loom-web      │ ─────────► │ docs-index.json │
│   .svx files    │  export    │                 │
└─────────────────┘            └────────┬────────┘
                                        │ startup
                                        ▼
┌─────────────────┐   GET      ┌─────────────────┐
│   DocSearch     │ ────────►  │  loom-server    │
│   component     │ /docs/     │  SQLite FTS5    │
│                 │  search    │  docs_fts table │
└─────────────────┘            └─────────────────┘
```

### 6.2 Build-time Export

At build time, `scripts/export-docs-index.ts` extracts doc content to JSON:

```json
{
  "version": 1,
  "generated_at": "2026-01-03T...",
  "docs": [
    {
      "doc_id": "tutorials/getting-started",
      "path": "/docs/tutorials/getting-started",
      "title": "Getting Started with Loom",
      "summary": "Create your first thread...",
      "diataxis": "tutorial",
      "tags": ["onboarding", "beginner"],
      "body": "Welcome to Loom! This tutorial..."
    }
  ]
}
```

Output: `static/docs-index.json` (bundled with loom-web build)

### 6.3 Server-side Indexing

On startup, loom-server loads `docs-index.json` into SQLite FTS5:

- **Table**: `docs_fts` (virtual FTS5 table)
- **Tokenizer**: unicode61 with prefix indexing
- **Ranking**: BM25
- **Highlighting**: FTS5 `snippet()` function

### 6.4 Search API

**Endpoint**: `GET /docs/search`

**Query Parameters**:

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `q` | string | yes | Search query |
| `diataxis` | enum | no | Filter: `tutorial`, `how-to`, `reference`, `explanation` |
| `limit` | u32 | no | Max results (default: 20, max: 50) |
| `offset` | u32 | no | Pagination offset |

**Response**:

```json
{
  "hits": [
    {
      "path": "/docs/tutorials/getting-started",
      "title": "Getting Started with Loom",
      "summary": "Create your first thread...",
      "diataxis": "tutorial",
      "tags": "onboarding beginner",
      "snippet": "A <mark>thread</mark> is a conversation with the Loom AI agent…",
      "score": 0.89
    }
  ],
  "limit": 20,
  "offset": 0
}
```

### 6.5 Search UI

- **Keyboard shortcut**: `Cmd/Ctrl + K`
- **Debounced input**: 200ms delay
- **Keyboard navigation**: Arrow keys + Enter
- **Category badges**: Shows Diátaxis type per result
- **Highlighted excerpts**: `<mark>` tags around matches

---

## 7. Syntax Highlighting

### 7.1 Shiki Configuration

```typescript
// svelte.config.js
import rehypePrettyCode from 'rehype-pretty-code';

const prettyCodeOptions = {
  theme: 'threadwork-dark',  // Custom theme
  keepBackground: false,
  defaultLang: 'plaintext',
};
```

### 7.2 Threadwork Syntax Theme

Custom Shiki theme using natural dye palette:

| Token | Color | Dye Reference |
|-------|-------|---------------|
| Keywords | `#7B9BC7` | Faded Indigo |
| Strings | `#4A7C59` | Lichen Green |
| Numbers | `#C9A227` | Weld Gold |
| Comments | `#6B6560` | Thread Silver |
| Functions | `#4A6FA5` | Woad Blue |
| Types | `#8B3A62` | Cochineal |
| Operators | `#9C9590` | fg-muted |
| Variables | `#F7F4F0` | Raw Linen (fg) |
| Errors | `#A63D2F` | Madder Red |

---

## 8. Build Configuration

### 8.1 svelte.config.js

```javascript
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { mdsvex } from 'mdsvex';
import remarkGfm from 'remark-gfm';
import rehypeSlug from 'rehype-slug';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import rehypePrettyCode from 'rehype-pretty-code';

const mdsvexConfig = {
  extensions: ['.svx'],
  remarkPlugins: [remarkGfm],
  rehypePlugins: [
    rehypeSlug,
    [rehypeAutolinkHeadings, { behavior: 'wrap' }],
    [rehypePrettyCode, { theme: 'github-dark' }],
  ],
};

export default {
  extensions: ['.svelte', '.svx'],
  preprocess: [mdsvex(mdsvexConfig), vitePreprocess()],
  kit: {
    adapter: adapter(),
  },
};
```

### 8.2 Dependencies

```json
{
  "devDependencies": {
    "mdsvex": "^0.12.0",
    "remark-gfm": "^4.0.0",
    "rehype-slug": "^6.0.0",
    "rehype-autolink-headings": "^7.0.0",
    "rehype-pretty-code": "^0.14.0",
    "shiki": "^1.0.0",
    "zod": "^3.23.0",
    "pagefind": "^1.0.0"
  }
}
```

---

## 9. Styling

### 9.1 Design Tokens

Uses Threadwork design system tokens:

- Typography: `--font-mono` (Commit Mono), `--text-base` (16px)
- Colors: Natural dye palette (`--color-info`, `--color-warning`, etc.)
- Spacing: `--space-*` scale
- Borders: `--color-border`, `--radius-md`
- Thread dividers for section separation

### 9.2 Layout

```
┌─────────────────────────────────────────────────────────────┐
│  Header: Logo + Search (Cmd+K)                              │
├────────────────┬────────────────────────────┬───────────────┤
│                │                            │               │
│   Sidebar      │   Main Content             │   TOC         │
│   (nav)        │   (article)                │   (aside)     │
│                │                            │               │
│   - Tutorials  │   Breadcrumbs              │   On this     │
│   - How-to     │   Title                    │   page:       │
│   - Reference  │   Content...               │   - Section 1 │
│   - Explain    │                            │   - Section 2 │
│                │   Prev/Next                │               │
│                │                            │               │
└────────────────┴────────────────────────────┴───────────────┘
```

### 9.3 Responsive Behavior

- Desktop: 3-column layout (sidebar, content, TOC)
- Tablet: 2-column (collapsible sidebar, content)
- Mobile: Single column, hamburger menu for nav

---

## 10. File Structure

```
src/
  lib/
    docs/
      schema.ts           # Zod schemas
      content.ts          # Content layer utilities
      nav.ts              # Navigation helpers
      index.ts            # Re-exports
      
      components/
        Callout.svelte
        CodeBlock.svelte
        AsciinemaPlayer.svelte
        Tabs.svelte
        TabItem.svelte
        LinkCard.svelte
        Steps.svelte
        Step.svelte
        TableOfContents.svelte
        Breadcrumbs.svelte
        PrevNext.svelte
        DocSearch.svelte
        index.ts
        
      themes/
        threadwork-dark.json    # Shiki theme
        threadwork-light.json
        
  routes/
    docs/
      +layout.svelte
      +layout.ts
      +page.svelte
      
      tutorials/
        +page.svelte
        getting-started/+page.svx
        first-thread/+page.svx
        
      how-to/
        +page.svelte
        ...
        
      reference/
        +page.svelte
        ...
        
      explanation/
        +page.svelte
        ...
```

---

## 11. Implementation Checklist

- [ ] Add mdsvex, remark-gfm, rehype-slug, rehype-autolink-headings, rehype-pretty-code, pagefind, zod dependencies
- [ ] Configure svelte.config.js with mdsvex and rehype plugins
- [ ] Create `$lib/docs/schema.ts` with Zod schemas
- [ ] Create `$lib/docs/content.ts` with content layer utilities
- [ ] Create `/docs` route structure with layouts
- [ ] Implement sidebar navigation with auto-generation
- [ ] Implement TOC extraction and display
- [ ] Implement breadcrumbs and prev/next navigation
- [ ] Create Callout component with variants
- [ ] Create CodeBlock component with copy button
- [ ] Create AsciinemaPlayer component
- [ ] Create Tabs/TabItem components
- [ ] Create LinkCard component
- [ ] Create Steps/Step components
- [ ] Create DocSearch component with Pagefind
- [ ] Create Threadwork Shiki themes
- [ ] Add Storybook stories for all components
- [ ] Create sample documentation pages
- [ ] Add postbuild script for Pagefind indexing

---

## Appendix A: Diátaxis Framework

| Category | Purpose | User Need |
|----------|---------|-----------|
| **Tutorials** | Learning-oriented | "I want to learn" |
| **How-to Guides** | Task-oriented | "I want to accomplish X" |
| **Reference** | Information-oriented | "I want to look up Y" |
| **Explanation** | Understanding-oriented | "I want to understand Z" |

Reference: https://diataxis.fr/

---

## Appendix B: Component Usage in MDX

```mdx
---
title: Example Documentation Page
summary: Shows how to use all documentation components.
diataxis: tutorial
order: 1
---

<script>
import { Callout, Steps, Step, Tabs, TabItem, LinkCard, AsciinemaPlayer } from '$lib/docs';
</script>

# Getting Started

<Callout variant="info" title="Prerequisites">
  You need Node.js 20+ installed.
</Callout>

<Steps>
  <Step title="Install">
    Install the CLI using your preferred package manager.
  </Step>
  <Step title="Configure">
    Set up your authentication credentials.
  </Step>
</Steps>

<Tabs>
  <TabItem label="pnpm">
    ```bash
    pnpm add -g loom-cli
    ```
  </TabItem>
  <TabItem label="npm">
    ```bash
    npm install -g loom-cli
    ```
  </TabItem>
</Tabs>

<AsciinemaPlayer id="abc123" autoplay />

<LinkCard 
  href="/docs/reference/cli"
  title="CLI Reference"
  description="Complete command reference for loom-cli"
/>
```
