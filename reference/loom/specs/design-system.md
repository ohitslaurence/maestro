<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Loom Design System: "Threadwork"

**Status:** Draft  
**Version:** 1.0  
**Last Updated:** 2026-01-02

---

## 1. Overview

A monospace-first design system for Loom, evoking the craft of textile weaving with warm, natural dye tones and subtle structural motifs. The design balances industrial precision with handcrafted warmth.

### Philosophy

- **Neutral utility** — The interface stays out of the way, like Commit Mono's design philosophy
- **Warm craft** — Natural dye colors reference historical textile traditions
- **Subtle structure** — Thread dividers and loom frame corners provide elegant visual hierarchy
- **Comfortable density** — 16px base, balanced whitespace for extended use

### Terminology

| Technical | Loom Metaphor |
|-----------|---------------|
| Conversation thread | Thread |
| Agent | Weaver |
| Tool execution | Shuttle pass |
| Message | Strand |
| Session | Pattern |
| Active state | Weaving |
| Error state | Broken thread |

---

## 2. Typography

### Font Stack

```css
--font-mono: 'CommitMono', 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
--font-sans: 'CommitMono', system-ui, sans-serif; /* Monospace throughout */
```

### Type Scale (Comfortable Density)

| Token | Size | Line Height | Weight | Use |
|-------|------|-------------|--------|-----|
| `--text-xs` | 12px | 1.5 | 400 | Captions, timestamps |
| `--text-sm` | 14px | 1.5 | 400 | Secondary text, labels |
| `--text-base` | 16px | 1.6 | 400 | Body text, messages |
| `--text-lg` | 18px | 1.5 | 500 | Section headers |
| `--text-xl` | 20px | 1.4 | 600 | Page titles |
| `--text-2xl` | 24px | 1.3 | 600 | Hero text |

### Font Loading

```html
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Commit+Mono:wght@400;500;600;700&display=swap" rel="stylesheet">
```

Or self-host from [commitmono.com](https://commitmono.com/).

---

## 3. Color Palette

Natural dye tones inspired by historical textile dyes: indigo, madder root, weld, lichen, raw linen.

### 3.1 Core Colors

#### Dark Mode (Primary)

```css
:root {
  /* Background - Loom Black to Shuttle Gray */
  --color-bg: #0D0C0B;
  --color-bg-muted: #1A1816;
  --color-bg-subtle: #2D2926;
  
  /* Foreground - Thread Silver to Raw Linen */
  --color-fg: #F7F4F0;
  --color-fg-muted: #9C9590;
  --color-fg-subtle: #6B6560;
  
  /* Accent - Indigo Thread */
  --color-accent: #7B9BC7;
  --color-accent-soft: #1E2A3D;
  --color-accent-hover: #4A6FA5;
  
  /* Border */
  --color-border: #3D3632;
  --color-border-muted: #2D2926;
  
  /* Thread (for dividers) */
  --color-thread: #4A6FA5;
  --color-thread-muted: rgba(74, 111, 165, 0.3);
}
```

#### Light Mode

```css
.light {
  /* Background - Raw Linen to Woven Cream */
  --color-bg: #F7F4F0;
  --color-bg-muted: #EDE8E3;
  --color-bg-subtle: #E3DDD6;
  
  /* Foreground - Spindle Brown */
  --color-fg: #3D3632;
  --color-fg-muted: #6B6560;
  --color-fg-subtle: #9C9590;
  
  /* Accent - Deep Indigo */
  --color-accent: #2C3E6B;
  --color-accent-soft: #E8ECF4;
  --color-accent-hover: #4A6FA5;
  
  /* Border */
  --color-border: #D4CEC7;
  --color-border-muted: #E3DDD6;
  
  /* Thread */
  --color-thread: #2C3E6B;
  --color-thread-muted: rgba(44, 62, 107, 0.3);
}
```

### 3.2 Status Colors (Natural Dyes)

```css
:root {
  /* Madder Red - Error */
  --color-error: #A63D2F;
  --color-error-soft: #2D1A17;
  
  /* Weld Gold - Warning */
  --color-warning: #C9A227;
  --color-warning-soft: #2D2615;
  
  /* Lichen Green - Success */
  --color-success: #4A7C59;
  --color-success-soft: #1A2D1F;
  
  /* Woad Blue - Info */
  --color-info: #4A6FA5;
  --color-info-soft: #1E2A3D;
}

.light {
  --color-error: #A63D2F;
  --color-error-soft: #F9EFED;
  
  --color-warning: #9A7B1E;
  --color-warning-soft: #FBF6E8;
  
  --color-success: #3D6B4A;
  --color-success-soft: #EDF5EF;
  
  --color-info: #2C3E6B;
  --color-info-soft: #E8ECF4;
}
```

### 3.3 Weaver Identity Colors

Each weaver (agent session) receives a distinct thread color for visual identity:

```css
:root {
  --weaver-indigo: #4A6FA5;
  --weaver-madder: #A63D2F;
  --weaver-weld: #C9A227;
  --weaver-lichen: #4A7C59;
  --weaver-cochineal: #8B3A62;
  --weaver-walnut: #6B5344;
  --weaver-copper: #7B6B4E;
  --weaver-iron: #4A4A4A;
}
```

---

## 4. Spacing & Layout

### Spacing Scale

```css
:root {
  --space-0: 0;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;
}
```

### Border Radius (Slightly Softened)

```css
:root {
  --radius-none: 0;
  --radius-sm: 2px;
  --radius-md: 4px;   /* Primary radius */
  --radius-lg: 6px;
  --radius-xl: 8px;
  --radius-full: 9999px;
}
```

### Shadows

```css
:root {
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.1);
  --shadow-md: 0 2px 4px rgba(0, 0, 0, 0.1), 0 1px 2px rgba(0, 0, 0, 0.06);
  --shadow-lg: 0 4px 8px rgba(0, 0, 0, 0.12), 0 2px 4px rgba(0, 0, 0, 0.08);
}
```

---

## 5. Visual Motifs

### 5.1 Thread Dividers

Single horizontal lines as section separators, replacing traditional borders.

```css
.thread-divider {
  height: 1px;
  background: linear-gradient(
    90deg,
    transparent 0%,
    var(--color-thread-muted) 10%,
    var(--color-thread-muted) 90%,
    transparent 100%
  );
  margin: var(--space-6) 0;
}

/* With knot accent */
.thread-divider-knot {
  position: relative;
  height: 1px;
  background: var(--color-thread-muted);
  margin: var(--space-6) 0;
}

.thread-divider-knot::after {
  content: '';
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 6px;
  height: 6px;
  background: var(--color-thread);
  border-radius: var(--radius-full);
}
```

### 5.2 Loom Frame Corners

Decorative L-shaped brackets for featured panels.

```css
.loom-frame {
  position: relative;
  padding: var(--space-6);
}

.loom-frame::before,
.loom-frame::after {
  content: '';
  position: absolute;
  width: 16px;
  height: 16px;
  border: 2px solid var(--color-thread-muted);
}

/* Top-left corner */
.loom-frame::before {
  top: 0;
  left: 0;
  border-right: none;
  border-bottom: none;
}

/* Bottom-right corner */
.loom-frame::after {
  bottom: 0;
  right: 0;
  border-left: none;
  border-top: none;
}

/* Four corners variant */
.loom-frame-full {
  position: relative;
}

.loom-frame-full .corner {
  position: absolute;
  width: 12px;
  height: 12px;
  border: 2px solid var(--color-thread-muted);
}

.loom-frame-full .corner-tl { top: 0; left: 0; border-right: none; border-bottom: none; }
.loom-frame-full .corner-tr { top: 0; right: 0; border-left: none; border-bottom: none; }
.loom-frame-full .corner-bl { bottom: 0; left: 0; border-right: none; border-top: none; }
.loom-frame-full .corner-br { bottom: 0; right: 0; border-left: none; border-top: none; }
```

---

## 6. Weaver State Animations

Visual feedback for agent states using thread metaphors.

### 6.1 State Definitions

| State | Visual | Animation |
|-------|--------|-----------|
| Idle | Static thread line | Subtle pulse (opacity) |
| Weaving | Thread being drawn | Horizontal shimmer |
| Waiting | Thread sway | Gentle oscillation |
| Error | Broken thread | Snap + fade fragments |
| Complete | Knot forming | Tie-off animation |

### 6.2 CSS Animations

```css
/* Idle - subtle pulse */
@keyframes thread-idle {
  0%, 100% { opacity: 0.6; }
  50% { opacity: 1; }
}

.weaver-idle .thread-indicator {
  animation: thread-idle 3s ease-in-out infinite;
}

/* Weaving - horizontal shimmer */
@keyframes thread-weaving {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}

.weaver-weaving .thread-indicator {
  background: linear-gradient(
    90deg,
    var(--color-thread-muted) 0%,
    var(--color-thread) 50%,
    var(--color-thread-muted) 100%
  );
  background-size: 200% 100%;
  animation: thread-weaving 1.5s ease-in-out infinite;
}

/* Waiting - gentle sway */
@keyframes thread-waiting {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(-2px); }
}

.weaver-waiting .thread-indicator {
  animation: thread-waiting 2s ease-in-out infinite;
}

/* Error - snap effect */
@keyframes thread-snap {
  0% { transform: scaleX(1); opacity: 1; }
  20% { transform: scaleX(1.1); }
  40% { transform: scaleX(0.5); opacity: 0.8; }
  60% { transform: scaleX(0); opacity: 0; }
  100% { transform: scaleX(0); opacity: 0; }
}

.weaver-error .thread-indicator {
  animation: thread-snap 0.5s ease-out forwards;
  background: var(--color-error);
}

/* Complete - knot tie-off */
@keyframes thread-complete {
  0% { width: 100%; }
  50% { width: 50%; }
  100% { 
    width: 8px; 
    height: 8px; 
    border-radius: var(--radius-full);
  }
}

.weaver-complete .thread-indicator {
  animation: thread-complete 0.6s ease-out forwards;
  background: var(--color-success);
}
```

### 6.3 Thread Indicator Component

```svelte
<script>
  let { state = 'idle', color = 'var(--weaver-indigo)' } = $props();
</script>

<div 
  class="thread-indicator weaver-{state}"
  style="--thread-color: {color}"
>
  <div class="thread-line"></div>
</div>

<style>
  .thread-indicator {
    width: 100%;
    height: 2px;
    position: relative;
    --color-thread: var(--thread-color);
  }
  
  .thread-line {
    width: 100%;
    height: 100%;
    background: var(--color-thread);
    border-radius: var(--radius-full);
  }
</style>
```

---

## 7. Component Patterns

### 7.1 Card

```svelte
<div class="card">
  <div class="card-header">
    <slot name="header" />
  </div>
  <div class="thread-divider"></div>
  <div class="card-body">
    <slot />
  </div>
</div>

<style>
  .card {
    background: var(--color-bg-muted);
    border: 1px solid var(--color-border-muted);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  
  .card-header {
    padding: var(--space-4);
    font-weight: 500;
  }
  
  .card-body {
    padding: var(--space-4);
  }
</style>
```

### 7.2 Button

```svelte
<script>
  let { variant = 'primary', size = 'md' } = $props();
</script>

<button class="btn btn-{variant} btn-{size}">
  <slot />
</button>

<style>
  .btn {
    font-family: var(--font-mono);
    font-weight: 500;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  
  .btn-md {
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
  }
  
  .btn-primary {
    background: var(--color-accent);
    color: var(--color-bg);
    border: 1px solid var(--color-accent);
  }
  
  .btn-primary:hover {
    background: var(--color-accent-hover);
  }
  
  .btn-secondary {
    background: transparent;
    color: var(--color-fg);
    border: 1px solid var(--color-border);
  }
  
  .btn-secondary:hover {
    background: var(--color-bg-subtle);
  }
  
  .btn-ghost {
    background: transparent;
    color: var(--color-fg-muted);
    border: 1px solid transparent;
  }
  
  .btn-ghost:hover {
    color: var(--color-fg);
    background: var(--color-bg-subtle);
  }
</style>
```

### 7.3 Badge (Weaver State)

```svelte
<script>
  let { state, weaverColor } = $props();
  
  const stateLabels = {
    idle: 'Idle',
    weaving: 'Weaving',
    waiting: 'Waiting',
    error: 'Broken Thread',
    complete: 'Complete'
  };
</script>

<span class="badge badge-{state}" style="--weaver-color: {weaverColor}">
  <span class="badge-dot"></span>
  {stateLabels[state]}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    font-family: var(--font-mono);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    color: var(--color-fg-muted);
  }
  
  .badge-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--weaver-color, var(--color-thread));
  }
  
  .badge-weaving .badge-dot {
    animation: thread-idle 1s ease-in-out infinite;
  }
  
  .badge-error {
    background: var(--color-error-soft);
    color: var(--color-error);
  }
  
  .badge-error .badge-dot {
    background: var(--color-error);
  }
  
  .badge-complete {
    background: var(--color-success-soft);
    color: var(--color-success);
  }
  
  .badge-complete .badge-dot {
    background: var(--color-success);
  }
</style>
```

---

## 8. Tailwind Configuration

```javascript
// tailwind.config.cjs
module.exports = {
  darkMode: 'class',
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      fontFamily: {
        mono: ['CommitMono', 'JetBrains Mono', 'Fira Code', 'ui-monospace', 'monospace'],
        sans: ['CommitMono', 'system-ui', 'sans-serif'],
      },
      colors: {
        // Core
        bg: 'var(--color-bg)',
        'bg-muted': 'var(--color-bg-muted)',
        'bg-subtle': 'var(--color-bg-subtle)',
        fg: 'var(--color-fg)',
        'fg-muted': 'var(--color-fg-muted)',
        'fg-subtle': 'var(--color-fg-subtle)',
        accent: 'var(--color-accent)',
        'accent-soft': 'var(--color-accent-soft)',
        'accent-hover': 'var(--color-accent-hover)',
        border: 'var(--color-border)',
        'border-muted': 'var(--color-border-muted)',
        thread: 'var(--color-thread)',
        'thread-muted': 'var(--color-thread-muted)',
        
        // Status
        error: 'var(--color-error)',
        'error-soft': 'var(--color-error-soft)',
        warning: 'var(--color-warning)',
        'warning-soft': 'var(--color-warning-soft)',
        success: 'var(--color-success)',
        'success-soft': 'var(--color-success-soft)',
        info: 'var(--color-info)',
        'info-soft': 'var(--color-info-soft)',
        
        // Weaver colors
        'weaver-indigo': 'var(--weaver-indigo)',
        'weaver-madder': 'var(--weaver-madder)',
        'weaver-weld': 'var(--weaver-weld)',
        'weaver-lichen': 'var(--weaver-lichen)',
        'weaver-cochineal': 'var(--weaver-cochineal)',
        'weaver-walnut': 'var(--weaver-walnut)',
        'weaver-copper': 'var(--weaver-copper)',
        'weaver-iron': 'var(--weaver-iron)',
      },
      borderRadius: {
        none: '0',
        sm: 'var(--radius-sm)',
        md: 'var(--radius-md)',
        lg: 'var(--radius-lg)',
        xl: 'var(--radius-xl)',
        full: 'var(--radius-full)',
      },
      spacing: {
        0: '0',
        1: 'var(--space-1)',
        2: 'var(--space-2)',
        3: 'var(--space-3)',
        4: 'var(--space-4)',
        5: 'var(--space-5)',
        6: 'var(--space-6)',
        8: 'var(--space-8)',
        10: 'var(--space-10)',
        12: 'var(--space-12)',
        16: 'var(--space-16)',
      },
      fontSize: {
        xs: ['var(--text-xs)', { lineHeight: '1.5' }],
        sm: ['var(--text-sm)', { lineHeight: '1.5' }],
        base: ['var(--text-base)', { lineHeight: '1.6' }],
        lg: ['var(--text-lg)', { lineHeight: '1.5' }],
        xl: ['var(--text-xl)', { lineHeight: '1.4' }],
        '2xl': ['var(--text-2xl)', { lineHeight: '1.3' }],
      },
      boxShadow: {
        sm: 'var(--shadow-sm)',
        md: 'var(--shadow-md)',
        lg: 'var(--shadow-lg)',
      },
      animation: {
        'thread-idle': 'thread-idle 3s ease-in-out infinite',
        'thread-weaving': 'thread-weaving 1.5s ease-in-out infinite',
        'thread-waiting': 'thread-waiting 2s ease-in-out infinite',
        'thread-snap': 'thread-snap 0.5s ease-out forwards',
        'thread-complete': 'thread-complete 0.6s ease-out forwards',
      },
    },
  },
  plugins: [],
};
```

---

## 9. Implementation Checklist

- [ ] Add Commit Mono font files to `/static/fonts/`
- [ ] Update `app.css` with new CSS variables
- [ ] Update `tailwind.config.cjs` with design tokens
- [ ] Create `ThreadDivider.svelte` component
- [ ] Create `LoomFrame.svelte` component
- [ ] Create `WeaverStateBadge.svelte` component
- [ ] Update existing components with new color tokens
- [ ] Add weaver thread color assignment logic
- [ ] Test light/dark mode transitions
- [ ] Add Storybook stories for new components

---

## Appendix: Color Reference

### Natural Dye Inspirations

| Color Name | Hex | Historical Dye |
|------------|-----|----------------|
| Loom Black | #0D0C0B | Iron gall ink |
| Charcoal Warp | #1A1816 | Charred wood |
| Shuttle Gray | #2D2926 | Iron mordant |
| Thread Silver | #9C9590 | Aged linen |
| Raw Linen | #F7F4F0 | Unbleached cotton |
| Bleached Cotton | #EDE8E3 | Sun-bleached fiber |
| Woven Cream | #E3DDD6 | Beeswax finish |
| Spindle Brown | #3D3632 | Walnut hull |
| Deep Indigo | #2C3E6B | Indigofera tinctoria |
| Woad Blue | #4A6FA5 | Isatis tinctoria |
| Faded Indigo | #7B9BC7 | Worn denim |
| Madder Red | #A63D2F | Rubia tinctorum |
| Weld Gold | #C9A227 | Reseda luteola |
| Lichen Green | #4A7C59 | Evernia prunastri |
| Cochineal | #8B3A62 | Dactylopius coccus |
| Walnut | #6B5344 | Juglans nigra |
