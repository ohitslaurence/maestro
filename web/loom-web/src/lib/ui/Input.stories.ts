/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import Input from './Input.svelte';

interface InputProps {
	value?: string;
	placeholder?: string;
	type?: 'text' | 'email' | 'password' | 'search';
	disabled?: boolean;
	error?: string;
	label?: string;
	id?: string;
	oninput?: (event: Event) => void;
	onkeydown?: (event: KeyboardEvent) => void;
}

const meta = {
	title: 'UI/Input',
	component: Input,
	tags: ['autodocs'],
	argTypes: {
		type: {
			control: 'select',
			options: ['text', 'email', 'password', 'search'],
		},
		disabled: { control: 'boolean' },
	},
} as Meta<InputProps>;

export default meta;
type Story = StoryObj<InputProps>;

export const Default: Story = {
	args: {
		placeholder: 'Enter text...',
	},
};

export const WithLabel: Story = {
	args: {
		label: 'Email Address',
		placeholder: 'name@example.com',
		type: 'email',
	},
};

export const WithError: Story = {
	args: {
		label: 'Username',
		placeholder: 'Enter username',
		error: 'Username is required',
	},
};

export const Disabled: Story = {
	args: {
		label: 'Disabled Input',
		placeholder: 'Cannot edit',
		disabled: true,
	},
};

export const Search: Story = {
	args: {
		type: 'search',
		placeholder: 'Search...',
	},
};
