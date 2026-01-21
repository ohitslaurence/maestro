// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import { EventEmitter } from 'events';
import { ChildProcess, spawn } from 'child_process';
import { Readable, Writable } from 'stream';

export interface IConfigService {
	readonly loomPath: string;
	getLoomCommand(): string[];
}

export interface IWorkspaceService {
	getWorkspaceRoot(): string | undefined;
}

export interface ILogger {
	info(message: string, ...args: unknown[]): void;
	warn(message: string, ...args: unknown[]): void;
	error(message: string, ...args: unknown[]): void;
	debug(message: string, ...args: unknown[]): void;
}

export interface AgentProcessManagerEvents {
	ready: [];
	exit: [code: number | null, signal: NodeJS.Signals | null];
	error: [error: Error];
	maxRestartsReached: [];
}

export class AgentProcessManager extends EventEmitter {
	private process: ChildProcess | null = null;
	private restartAttempts = 0;
	private restartTimeoutId: NodeJS.Timeout | null = null;
	private isShuttingDown = false;

	private static readonly MAX_RESTART_ATTEMPTS = 10;
	private static readonly MAX_BACKOFF_MS = 30000;
	private static readonly INITIAL_BACKOFF_MS = 1000;

	constructor(
		private readonly configService: IConfigService,
		private readonly workspaceService: IWorkspaceService,
		private readonly logger: ILogger
	) {
		super();
	}

	public async start(): Promise<void> {
		if (this.process && this.isActive()) {
			this.logger.warn('Agent process already running, ignoring start request');
			return;
		}

		this.isShuttingDown = false;
		await this.spawnProcess();
	}

	public stop(): void {
		this.isShuttingDown = true;

		if (this.restartTimeoutId) {
			clearTimeout(this.restartTimeoutId);
			this.restartTimeoutId = null;
		}

		if (this.process) {
			this.logger.info('Stopping agent process', { pid: this.process.pid });
			this.process.kill('SIGTERM');
			this.process = null;
		}

		this.restartAttempts = 0;
	}

	public async restart(): Promise<void> {
		this.logger.info('Restarting agent process');
		this.stop();
		this.restartAttempts = 0;
		await this.start();
	}

	public getProcess(): ChildProcess | null {
		return this.process;
	}

	public getStdin(): Writable | null {
		return this.process?.stdin ?? null;
	}

	public getStdout(): Readable | null {
		return this.process?.stdout ?? null;
	}

	public isActive(): boolean {
		return this.process !== null && !this.process.killed && this.process.exitCode === null;
	}

	private async spawnProcess(): Promise<void> {
		const binaryPath = this.configService.loomPath;
		const command = this.configService.getLoomCommand();
		const workspaceRoot = this.workspaceService.getWorkspaceRoot();

		this.logger.info('Starting agent process', { binaryPath, command, workspaceRoot });

		try {
			this.process = spawn(binaryPath, command, {
				stdio: ['pipe', 'pipe', 'pipe'],
				cwd: workspaceRoot,
			});

			this.logger.info('Agent process started', { pid: this.process.pid });

			this.process.on('error', (error: NodeJS.ErrnoException) => {
				if (error.code === 'ENOENT') {
					const message = `Loom binary not found at '${binaryPath}'. Please ensure loom is installed and the path is correct.`;
					this.logger.error(message);
					this.emit('error', new Error(message));
				} else {
					this.logger.error('Agent process error', { error: error.message });
					this.emit('error', error);
				}
			});

			this.process.on('exit', (code, signal) => {
				this.logger.info('Agent process exited', { code, signal, pid: this.process?.pid });
				this.emit('exit', code, signal);
				this.process = null;

				if (!this.isShuttingDown) {
					this.scheduleRestart();
				}
			});

			this.process.stderr?.on('data', (data: Buffer) => {
				this.logger.warn('Agent stderr', { output: data.toString() });
			});

			this.restartAttempts = 0;
			this.emit('ready');
		} catch (error) {
			const err = error instanceof Error ? error : new Error(String(error));
			this.logger.error('Failed to spawn agent process', { error: err.message });
			this.emit('error', err);
			this.scheduleRestart();
		}
	}

	private scheduleRestart(): void {
		if (this.isShuttingDown) {
			return;
		}

		if (this.restartAttempts >= AgentProcessManager.MAX_RESTART_ATTEMPTS) {
			this.logger.error('Max restart attempts reached, giving up');
			this.emit('maxRestartsReached');
			return;
		}

		const backoffMs = Math.min(
			AgentProcessManager.INITIAL_BACKOFF_MS * Math.pow(2, this.restartAttempts),
			AgentProcessManager.MAX_BACKOFF_MS
		);

		this.restartAttempts++;
		this.logger.info('Scheduling agent restart', {
			attempt: this.restartAttempts,
			maxAttempts: AgentProcessManager.MAX_RESTART_ATTEMPTS,
			backoffMs,
		});

		this.restartTimeoutId = setTimeout(async () => {
			this.restartTimeoutId = null;
			await this.spawnProcess();
		}, backoffMs);
	}
}
