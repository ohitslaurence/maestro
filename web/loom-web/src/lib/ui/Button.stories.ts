/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import type { Snippet } from 'svelte';
import { createRawSnippet } from 'svelte';
import Button from './Button.svelte';

interface ButtonProps {
	variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'warning' | 'success';
	size?: 'sm' | 'md' | 'lg';
	disabled?: boolean;
	loading?: boolean;
	type?: 'button' | 'submit' | 'reset';
	onclick?: (event: MouseEvent) => void;
	class?: string;
	children: Snippet;
}

const meta = {
	title: 'UI/Button',
	component: Button,
	tags: ['autodocs'],
	argTypes: {
		variant: {
			control: 'select',
			options: ['primary', 'secondary', 'ghost', 'danger', 'warning', 'success'],
		},
		size: {
			control: 'select',
			options: ['sm', 'md', 'lg'],
		},
		disabled: { control: 'boolean' },
		loading: { control: 'boolean' },
	},
} as Meta<ButtonProps>;

export default meta;
type Story = StoryObj<ButtonProps>;

const createTextSnippet = (text: string) =>
	createRawSnippet(() => ({
		render: () => text,
	}));

export const Primary: Story = {
	args: {
		variant: 'primary',
		children: createTextSnippet('Primary Button'),
	},
};

export const Secondary: Story = {
	args: {
		variant: 'secondary',
		children: createTextSnippet('Secondary Button'),
	},
};

export const Ghost: Story = {
	args: {
		variant: 'ghost',
		children: createTextSnippet('Ghost Button'),
	},
};

export const Danger: Story = {
	args: {
		variant: 'danger',
		children: createTextSnippet('Danger Button'),
	},
};

export const Small: Story = {
	args: {
		size: 'sm',
		children: createTextSnippet('Small Button'),
	},
};

export const Large: Story = {
	args: {
		size: 'lg',
		children: createTextSnippet('Large Button'),
	},
};

export const Loading: Story = {
	args: {
		loading: true,
		children: createTextSnippet('Loading...'),
	},
};

export const Disabled: Story = {
	args: {
		disabled: true,
		children: createTextSnippet('Disabled Button'),
	},
};
