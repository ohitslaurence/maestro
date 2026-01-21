/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import ThreadDivider from './ThreadDivider.svelte';

const meta: Meta<typeof ThreadDivider> = {
	title: 'UI/ThreadDivider',
	component: ThreadDivider,
	tags: ['autodocs'],
	argTypes: {
		variant: {
			control: 'select',
			options: ['simple', 'gradient', 'knot'],
		},
	},
	decorators: [
		() => ({
			Component: undefined,
			template: '<div style="width: 400px; padding: 20px;"><story /></div>',
		}),
	],
};

export default meta;
type Story = StoryObj<typeof ThreadDivider>;

export const Simple: Story = {
	args: {
		variant: 'simple',
	},
};

export const Gradient: Story = {
	args: {
		variant: 'gradient',
	},
};

export const Knot: Story = {
	args: {
		variant: 'knot',
	},
};
