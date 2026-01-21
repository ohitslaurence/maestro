/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import LoomFrameStoryWrapper from './LoomFrameStoryWrapper.svelte';

type LoomFrameStoryProps = {
	variant?: 'corners' | 'full';
	text?: string;
};

const meta: Meta<LoomFrameStoryProps> = {
	title: 'UI/LoomFrame',
	component: LoomFrameStoryWrapper,
	tags: ['autodocs'],
	argTypes: {
		variant: {
			control: 'select',
			options: ['corners', 'full'],
		},
		text: {
			control: 'text',
		},
	},
};

export default meta;
type Story = StoryObj<LoomFrameStoryProps>;

export const Corners: Story = {
	args: {
		variant: 'corners',
		text: 'Featured content with corner brackets',
	},
};

export const Full: Story = {
	args: {
		variant: 'full',
		text: 'Featured content with all four corner brackets',
	},
};
