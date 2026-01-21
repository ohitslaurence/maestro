/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import type { Snippet } from 'svelte';
import { createRawSnippet } from 'svelte';
import Card from './Card.svelte';

interface CardProps {
	padding?: 'none' | 'sm' | 'md' | 'lg';
	hover?: boolean;
	showDivider?: boolean;
	header?: Snippet;
	footer?: Snippet;
	children: Snippet;
}

const meta = {
	title: 'UI/Card',
	component: Card,
	tags: ['autodocs'],
	argTypes: {
		padding: {
			control: 'select',
			options: ['none', 'sm', 'md', 'lg'],
		},
		hover: { control: 'boolean' },
		showDivider: { control: 'boolean' },
	},
} as Meta<CardProps>;

export default meta;
type Story = StoryObj<CardProps>;

export const Default: Story = {
	args: {
		padding: 'md',
		children: createRawSnippet(() => ({
			render: () => `<span>Card content goes here. This is a basic card with default padding.</span>`,
		})),
	},
};

export const WithHover: Story = {
	args: {
		padding: 'md',
		hover: true,
		children: createRawSnippet(() => ({
			render: () => `<span>Hover over this card to see the effect.</span>`,
		})),
	},
};

export const NoPadding: Story = {
	args: {
		padding: 'none',
		children: createRawSnippet(() => ({
			render: () => `<span>Card with no padding.</span>`,
		})),
	},
};

export const LargePadding: Story = {
	args: {
		padding: 'lg',
		children: createRawSnippet(() => ({
			render: () => `<span>Card with large padding for more spacious content.</span>`,
		})),
	},
};
