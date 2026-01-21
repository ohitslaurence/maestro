/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import LoginPage from '../../routes/login/+page.svelte';

vi.mock('$lib/i18n', () => ({
	i18n: {
		_: (key: string) => {
			const translations: Record<string, string> = {
				'auth.login.title': 'Sign In',
				'auth.login.subtitle': 'Sign in to your account',
				'auth.login.github': 'Continue with GitHub',
				'auth.login.google': 'Continue with Google',
				'auth.login.or': 'or',
				'auth.login.emailLabel': 'Email address',
				'auth.login.emailPlaceholder': 'you@example.com',
				'auth.login.sendMagicLink': 'Send magic link',
				'auth.login.sending': 'Sending...',
				'auth.login.checkEmail': 'Check your email',
				'auth.login.magicLinkSent': 'We sent a magic link to',
				'auth.login.useDifferentEmail': 'Use a different email',
				'auth.login.error': 'Failed to send magic link',
			};
			return translations[key] || key;
		},
	},
}));

const mockRequestMagicLink = vi.fn();
vi.mock('$lib/api/client', () => ({
	getApiClient: () => ({
		requestMagicLink: mockRequestMagicLink,
	}),
}));

describe('LoginPage', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockRequestMagicLink.mockReset();
	});

	it('renders OAuth buttons (GitHub, Google)', () => {
		render(LoginPage);

		expect(screen.getByText('Continue with GitHub')).toBeInTheDocument();
		expect(screen.getByText('Continue with Google')).toBeInTheDocument();
	});

	it('renders magic link form', () => {
		render(LoginPage);

		expect(screen.getByLabelText('Email address')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Send magic link' })).toBeInTheDocument();
	});

	it('submitting magic link form calls API', async () => {
		mockRequestMagicLink.mockResolvedValue({ success: true });
		render(LoginPage);

		const emailInput = screen.getByLabelText('Email address');
		await fireEvent.input(emailInput, { target: { value: 'test@example.com' } });

		const submitButton = screen.getByRole('button', { name: 'Send magic link' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(mockRequestMagicLink).toHaveBeenCalledWith('test@example.com');
		});
	});

	it('shows success message after magic link sent', async () => {
		mockRequestMagicLink.mockResolvedValue({ success: true });
		render(LoginPage);

		const emailInput = screen.getByLabelText('Email address');
		await fireEvent.input(emailInput, { target: { value: 'test@example.com' } });

		const submitButton = screen.getByRole('button', { name: 'Send magic link' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(screen.getByText('Check your email')).toBeInTheDocument();
		});
	});

	it('shows error message on API failure', async () => {
		mockRequestMagicLink.mockRejectedValue(new Error('Network error'));
		render(LoginPage);

		const emailInput = screen.getByLabelText('Email address');
		await fireEvent.input(emailInput, { target: { value: 'test@example.com' } });

		const submitButton = screen.getByRole('button', { name: 'Send magic link' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(screen.getByText('Failed to send magic link')).toBeInTheDocument();
		});
	});
});
