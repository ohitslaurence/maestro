/**
 * SSE Event Emitter (§4)
 *
 * Manages SSE client connections and broadcasts events to all connected clients.
 * Uses OpenCode-compatible event envelope: { type, properties }
 */

import { logger } from '../logger';
import type {
  MessagePartUpdatedEvent,
  MessageUpdatedEvent,
  PermissionAskedEvent,
  PermissionRepliedEvent,
  SSEEvent,
  SSEEventType,
  Session,
  SessionCreatedEvent,
  SessionStatusEvent,
  SessionUpdatedEvent,
} from '../types';

// --- Types ---

interface SSEClient {
  id: string;
  controller: ReadableStreamDefaultController<string>;
  connectedAt: number;
}

// --- SSE Emitter Singleton ---

class SSEEmitter {
  private clients: Map<string, SSEClient> = new Map();
  private clientIdCounter = 0;
  private keepAliveInterval: ReturnType<typeof setInterval> | null = null;

  constructor() {
    // Start keep-alive ping every 30s
    this.startKeepAlive();
  }

  /**
   * Register a new SSE client connection.
   * Returns a ReadableStream that the client can consume.
   */
  addClient(): { stream: ReadableStream<string>; clientId: string } {
    const clientId = `client-${++this.clientIdCounter}`;

    const stream = new ReadableStream<string>({
      start: (controller) => {
        const client: SSEClient = {
          id: clientId,
          controller,
          connectedAt: Date.now(),
        };
        this.clients.set(clientId, client);
        logger.debug('sse client connected', { clientId, totalClients: this.clients.size });

        // Send immediate heartbeat to prevent connection timeout
        const heartbeat: SSEEvent<object> = { type: 'server.heartbeat', properties: {} };
        const data = `data: ${JSON.stringify(heartbeat)}\n\n`;
        controller.enqueue(data);
      },
      cancel: () => {
        this.removeClient(clientId);
      },
    });

    return { stream, clientId };
  }

  /**
   * Remove a client connection.
   */
  removeClient(clientId: string): void {
    const client = this.clients.get(clientId);
    if (client) {
      this.clients.delete(clientId);
      try {
        client.controller.close();
      } catch {
        // Controller may already be closed
      }
      logger.debug('sse client disconnected', { clientId, totalClients: this.clients.size });
    }
  }

  /**
   * Get the number of connected clients.
   */
  getClientCount(): number {
    return this.clients.size;
  }

  /**
   * Emit an event to all connected clients.
   * Event format: { type, properties } per OpenCode spec (§4)
   */
  emit<T>(type: SSEEventType, properties: T): void {
    const event: SSEEvent<T> = { type, properties };
    const data = `data: ${JSON.stringify(event)}\n\n`;

    const deadClients: string[] = [];

    for (const [clientId, client] of this.clients) {
      try {
        client.controller.enqueue(data);
      } catch (err) {
        logger.warn('failed to send event to client', { clientId, error: String(err) });
        deadClients.push(clientId);
      }
    }

    // Clean up dead clients
    for (const clientId of deadClients) {
      this.removeClient(clientId);
    }

    logger.debug('sse event emitted', { type, clientCount: this.clients.size });
  }

  /**
   * Send a keep-alive comment to all clients.
   * SSE comment format: `:ping\n\n`
   */
  private sendKeepAlive(): void {
    if (this.clients.size === 0) return;

    const ping = `:ping ${Date.now()}\n\n`;
    const deadClients: string[] = [];

    for (const [clientId, client] of this.clients) {
      try {
        client.controller.enqueue(ping);
      } catch {
        deadClients.push(clientId);
      }
    }

    for (const clientId of deadClients) {
      this.removeClient(clientId);
    }
  }

  /**
   * Start the keep-alive interval (30s per spec).
   */
  private startKeepAlive(): void {
    if (this.keepAliveInterval) return;
    this.keepAliveInterval = setInterval(() => this.sendKeepAlive(), 30000);
  }

  /**
   * Stop the keep-alive interval.
   */
  stopKeepAlive(): void {
    if (this.keepAliveInterval) {
      clearInterval(this.keepAliveInterval);
      this.keepAliveInterval = null;
    }
  }

  /**
   * Close all client connections (for shutdown).
   */
  closeAll(): void {
    for (const clientId of Array.from(this.clients.keys())) {
      this.removeClient(clientId);
    }
    this.stopKeepAlive();
  }

  // --- Convenience methods for common events ---

  /**
   * Emit session.created event (§4)
   */
  emitSessionCreated(session: Session): void {
    const properties: SessionCreatedEvent = { info: session };
    this.emit('session.created', properties);
  }

  /**
   * Emit session.updated event (§4)
   */
  emitSessionUpdated(session: Session): void {
    const properties: SessionUpdatedEvent = { info: session };
    this.emit('session.updated', properties);
  }

  /**
   * Emit session.status event (§4)
   */
  emitSessionStatus(
    sessionId: string,
    status: { type: 'idle' | 'busy' | 'error'; attempt?: number; message?: string }
  ): void {
    const properties: SessionStatusEvent = { sessionId, status };
    this.emit('session.status', properties);
  }

  /**
   * Emit message.updated event (§4)
   */
  emitMessageUpdated(info: MessageUpdatedEvent['info']): void {
    const properties: MessageUpdatedEvent = { info };
    this.emit('message.updated', properties);
  }

  /**
   * Emit message.part.updated event (§4)
   */
  emitMessagePartUpdated(part: MessagePartUpdatedEvent['part'], delta?: string): void {
    const properties: MessagePartUpdatedEvent = { part, delta };
    this.emit('message.part.updated', properties);
  }

  /**
   * Emit permission.asked event (§4)
   */
  emitPermissionAsked(properties: PermissionAskedEvent): void {
    this.emit('permission.asked', properties);
  }

  /**
   * Emit permission.replied event (§4)
   */
  emitPermissionReplied(properties: PermissionRepliedEvent): void {
    this.emit('permission.replied', properties);
  }
}

// --- Singleton instance ---

export const sseEmitter = new SSEEmitter();
