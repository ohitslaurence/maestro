/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import WeaverStateBadge, { type WeaverState } from './WeaverStateBadge.svelte';

interface WeaverStateBadgeProps {
	state: WeaverState;
	weaverColor?: string;
	size?: 'sm' | 'md';
}

const meta: Meta<WeaverStateBadgeProps> = {
	title: 'UI/WeaverStateBadge',
	component: WeaverStateBadge as any,
	tags: ['autodocs'],
	argTypes: {
		state: {
			control: 'select',
			options: ['idle', 'weaving', 'waiting', 'error', 'complete'],
		},
		weaverColor: {
			control: 'select',
			options: [
				'var(--weaver-indigo)',
				'var(--weaver-madder)',
				'var(--weaver-weld)',
				'var(--weaver-lichen)',
				'var(--weaver-cochineal)',
				'var(--weaver-walnut)',
				'var(--weaver-copper)',
				'var(--weaver-iron)',
			],
		},
		size: {
			control: 'select',
			options: ['sm', 'md'],
		},
	},
};

export default meta;
type Story = StoryObj<WeaverStateBadgeProps>;

export const Idle: Story = {
	args: {
		state: 'idle',
		weaverColor: 'var(--weaver-indigo)',
		size: 'md',
	},
};

export const Weaving: Story = {
	args: {
		state: 'weaving',
		weaverColor: 'var(--weaver-indigo)',
		size: 'md',
	},
};

export const Waiting: Story = {
	args: {
		state: 'waiting',
		weaverColor: 'var(--weaver-indigo)',
		size: 'md',
	},
};

export const Error: Story = {
	args: {
		state: 'error',
		size: 'md',
	},
};

export const Complete: Story = {
	args: {
		state: 'complete',
		size: 'md',
	},
};

export const SmallSize: Story = {
	args: {
		state: 'weaving',
		weaverColor: 'var(--weaver-indigo)',
		size: 'sm',
	},
};

export const MadderColor: Story = {
	args: {
		state: 'weaving',
		weaverColor: 'var(--weaver-madder)',
		size: 'md',
	},
};

export const WeldColor: Story = {
	args: {
		state: 'idle',
		weaverColor: 'var(--weaver-weld)',
		size: 'md',
	},
};

export const LichenColor: Story = {
	args: {
		state: 'waiting',
		weaverColor: 'var(--weaver-lichen)',
		size: 'md',
	},
};

export const CochinealColor: Story = {
	args: {
		state: 'weaving',
		weaverColor: 'var(--weaver-cochineal)',
		size: 'md',
	},
};
