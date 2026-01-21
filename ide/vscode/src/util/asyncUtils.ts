// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

export function debounce<T extends (...args: unknown[]) => unknown>(
    fn: T,
    wait: number
): (...args: Parameters<T>) => void {
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    return (...args: Parameters<T>): void => {
        if (timeoutId !== undefined) {
            clearTimeout(timeoutId);
        }
        timeoutId = setTimeout(() => {
            fn(...args);
        }, wait);
    };
}

export function throttle<T extends (...args: unknown[]) => unknown>(
    fn: T,
    wait: number
): (...args: Parameters<T>) => void {
    let lastTime = 0;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    return (...args: Parameters<T>): void => {
        const now = Date.now();
        const remaining = wait - (now - lastTime);
        if (remaining <= 0) {
            if (timeoutId !== undefined) {
                clearTimeout(timeoutId);
                timeoutId = undefined;
            }
            lastTime = now;
            fn(...args);
        } else if (timeoutId === undefined) {
            timeoutId = setTimeout(() => {
                lastTime = Date.now();
                timeoutId = undefined;
                fn(...args);
            }, remaining);
        }
    };
}

export function withTimeout<T>(
    promise: Promise<T>,
    ms: number,
    message?: string
): Promise<T> {
    return new Promise((resolve, reject) => {
        const timeoutId = setTimeout(() => {
            reject(new Error(message ?? `Operation timed out after ${ms}ms`));
        }, ms);
        promise
            .then((result) => {
                clearTimeout(timeoutId);
                resolve(result);
            })
            .catch((error) => {
                clearTimeout(timeoutId);
                reject(error);
            });
    });
}

export function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

export interface RetryOptions {
    maxRetries: number;
    initialDelayMs: number;
    maxDelayMs: number;
}

export async function retryWithBackoff<T>(
    fn: () => Promise<T>,
    options: RetryOptions
): Promise<T> {
    const { maxRetries, initialDelayMs, maxDelayMs } = options;
    let lastError: Error | undefined;
    let delay = initialDelayMs;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
            return await fn();
        } catch (error) {
            lastError = error instanceof Error ? error : new Error(String(error));
            if (attempt < maxRetries) {
                await sleep(delay);
                delay = Math.min(delay * 2, maxDelayMs);
            }
        }
    }

    throw lastError;
}
