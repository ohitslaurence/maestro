/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import BadgeStoryWrapper from './BadgeStoryWrapper.svelte';

type BadgeStoryProps = {
	variant?: 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info' | 'muted';
	size?: 'sm' | 'md';
	text?: string;
};

const meta: Meta<BadgeStoryProps> = {
	title: 'UI/Badge',
	component: BadgeStoryWrapper,
	tags: ['autodocs'],
	argTypes: {
		variant: {
			control: 'select',
			options: ['default', 'accent', 'success', 'warning', 'error', 'info', 'muted'],
		},
		size: {
			control: 'select',
			options: ['sm', 'md'],
		},
		text: {
			control: 'text',
		},
	},
};

export default meta;
type Story = StoryObj<BadgeStoryProps>;

export const Default: Story = {
	args: {
		variant: 'default',
		text: 'Default',
	},
};

export const Accent: Story = {
	args: {
		variant: 'accent',
		text: 'Accent',
	},
};

export const Success: Story = {
	args: {
		variant: 'success',
		text: 'Success',
	},
};

export const Warning: Story = {
	args: {
		variant: 'warning',
		text: 'Warning',
	},
};

export const Error: Story = {
	args: {
		variant: 'error',
		text: 'Error',
	},
};

export const Muted: Story = {
	args: {
		variant: 'muted',
		text: 'Muted',
	},
};

export const Info: Story = {
	args: {
		variant: 'info',
		text: 'Info',
	},
};

export const Small: Story = {
	args: {
		size: 'sm',
		text: 'Small Badge',
	},
};
