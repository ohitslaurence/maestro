/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Meta, StoryObj } from '@storybook/svelte';
import AdminUserCard from './AdminUserCard.svelte';
import type { AdminUser } from '$lib/api/types';

interface AdminUserCardProps {
	user: AdminUser;
	currentUserId: string;
	onToggleSystemAdmin?: (userId: string, currentValue: boolean) => void;
	onImpersonate?: (userId: string) => void;
	isUpdating?: boolean;
	isImpersonating?: boolean;
}

const meta: Meta<AdminUserCardProps> = {
	title: 'UI/AdminUserCard',
	component: AdminUserCard as any,
	tags: ['autodocs'],
	argTypes: {
		onToggleSystemAdmin: { action: 'toggleSystemAdmin' },
		onImpersonate: { action: 'impersonate' },
	},
};

export default meta;
type Story = StoryObj<AdminUserCardProps>;

const baseUser: AdminUser = {
	id: 'user-123',
	display_name: 'John Doe',
	primary_email: 'john@example.com',
	avatar_url: null,
	is_system_admin: false,
	is_support: false,
	is_auditor: false,
	created_at: '2024-01-15T10:30:00Z',
	updated_at: '2025-01-02T14:22:00Z',
	deleted_at: null,
};

export const RegularUser: Story = {
	args: {
		user: { ...baseUser },
		currentUserId: 'other-user',
	},
};

export const SystemAdmin: Story = {
	args: {
		user: { ...baseUser, is_system_admin: true },
		currentUserId: 'other-user',
	},
};

export const AllRoles: Story = {
	args: {
		user: { ...baseUser, is_system_admin: true, is_support: true, is_auditor: true },
		currentUserId: 'other-user',
	},
};

export const CurrentUser: Story = {
	args: {
		user: { ...baseUser, is_system_admin: true },
		currentUserId: 'user-123',
	},
};

export const WithAvatar: Story = {
	args: {
		user: {
			...baseUser,
			avatar_url: 'https://avatars.githubusercontent.com/u/1?v=4',
			is_system_admin: true,
		},
		currentUserId: 'other-user',
	},
};

export const Updating: Story = {
	args: {
		user: { ...baseUser, is_system_admin: true },
		currentUserId: 'other-user',
		isUpdating: true,
	},
};
