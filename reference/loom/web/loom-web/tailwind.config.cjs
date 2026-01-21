/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/** @type {import('tailwindcss').Config} */
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
