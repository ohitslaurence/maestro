/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import DevicePage from '../../routes/device/+page.svelte';

vi.mock('$lib/i18n', () => ({
	i18n: {
		_: (key: string) => {
			const translations: Record<string, string> = {
				'auth.device.title': 'Authorize Device',
				'auth.device.subtitle': 'Enter the code displayed on your device',
				'auth.device.inputLabel': 'Device code',
				'auth.device.placeholder': 'XXX-XXX-XXX',
				'auth.device.authorize': 'Authorize',
				'auth.device.authorizing': 'Authorizing...',
				'auth.device.success': 'Device Authorized',
				'auth.device.successMessage': 'You can now close this page',
				'auth.device.error': 'Invalid or expired code',
				'auth.device.noCode': 'No code?',
				'auth.device.cliCommand': 'loom login',
				'auth.device.inTerminal': 'in your terminal',
			};
			return translations[key] || key;
		},
	},
}));

const mockCompleteDeviceCode = vi.fn();
vi.mock('$lib/api/client', () => ({
	getApiClient: () => ({
		completeDeviceCode: mockCompleteDeviceCode,
	}),
}));

describe('DevicePage', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockCompleteDeviceCode.mockReset();
	});

	it('renders code input', () => {
		render(DevicePage);

		expect(screen.getByLabelText('Device code')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Authorize' })).toBeInTheDocument();
	});

	it('formats code as XXX-XXX-XXX', async () => {
		render(DevicePage);

		const codeInput = screen.getByLabelText('Device code') as HTMLInputElement;
		await fireEvent.input(codeInput, { target: { value: 'abc123def' } });

		expect(codeInput.value).toBe('ABC-123-DEF');
	});

	it('submitting code calls API', async () => {
		mockCompleteDeviceCode.mockResolvedValue({ success: true });
		render(DevicePage);

		const codeInput = screen.getByLabelText('Device code');
		await fireEvent.input(codeInput, { target: { value: 'ABC-123-DEF' } });

		const submitButton = screen.getByRole('button', { name: 'Authorize' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(mockCompleteDeviceCode).toHaveBeenCalledWith('ABC-123-DEF');
		});
	});

	it('shows success message on authorization', async () => {
		mockCompleteDeviceCode.mockResolvedValue({ success: true });
		render(DevicePage);

		const codeInput = screen.getByLabelText('Device code');
		await fireEvent.input(codeInput, { target: { value: 'ABC-123-DEF' } });

		const submitButton = screen.getByRole('button', { name: 'Authorize' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(screen.getByText('Device Authorized')).toBeInTheDocument();
			expect(screen.getByText('You can now close this page')).toBeInTheDocument();
		});
	});

	it('shows error on invalid code', async () => {
		mockCompleteDeviceCode.mockRejectedValue(new Error('Invalid code'));
		render(DevicePage);

		const codeInput = screen.getByLabelText('Device code');
		await fireEvent.input(codeInput, { target: { value: 'ABC-123-DEF' } });

		const submitButton = screen.getByRole('button', { name: 'Authorize' });
		await fireEvent.click(submitButton);

		await waitFor(() => {
			expect(screen.getByText('Invalid or expired code')).toBeInTheDocument();
		});
	});
});
