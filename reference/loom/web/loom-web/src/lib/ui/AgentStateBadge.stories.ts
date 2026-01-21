/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import AgentStateBadge from './AgentStateBadge.svelte';
import type { AgentStateKind } from '../api/types';

interface AgentStateBadgeProps {
	state: AgentStateKind;
	weaverColor?: string;
	size?: 'sm' | 'md';
}

const meta: Meta<AgentStateBadgeProps> = {
	title: 'UI/AgentStateBadge',
	component: AgentStateBadge as any,
	tags: ['autodocs'],
	argTypes: {
		state: {
			control: 'select',
			options: [
				'idle',
				'thinking',
				'streaming',
				'tool_pending',
				'tool_executing',
				'waiting_input',
				'error',
			],
			description: 'The agent state (maps to Threadwork weaver terminology)',
		},
		weaverColor: {
			control: 'color',
			description: 'Custom weaver thread color',
		},
		size: {
			control: 'select',
			options: ['sm', 'md'],
		},
	},
};

export default meta;
type Story = StoryObj<AgentStateBadgeProps>;

export const Idle: Story = {
	args: {
		state: 'idle',
	},
};

export const Weaving: Story = {
	args: {
		state: 'thinking',
	},
};

export const Streaming: Story = {
	args: {
		state: 'streaming',
	},
};

export const ShuttlePass: Story = {
	args: {
		state: 'tool_executing',
	},
};

export const ToolPending: Story = {
	args: {
		state: 'tool_pending',
	},
};

export const Waiting: Story = {
	args: {
		state: 'waiting_input',
	},
};

export const BrokenThread: Story = {
	args: {
		state: 'error',
	},
};

export const SmallSize: Story = {
	args: {
		state: 'thinking',
		size: 'sm',
	},
};

export const CustomWeaverColor: Story = {
	args: {
		state: 'thinking',
		weaverColor: 'var(--weaver-madder)',
	},
};

export const WeaverColors: Story = {
	args: {
		state: 'thinking',
		weaverColor: 'var(--weaver-cochineal)',
	},
};
