import { WebSocketServer, WebSocket } from 'ws';
import { EventBus, EventKey, AgentEventMap } from '../core/event-bus.js';
import { logger } from '../core/logger.js';

export interface MonitorEvent {
  type: string;
  timestamp: number;
  agentId: string;
  data: unknown;
}

export interface SubscribeMessage {
  type: 'subscribe';
  events: string[];
}

export class MonitorServer {
  private wss: WebSocketServer | null = null;
  private clients: Set<WebSocket> = new Set();
  private eventBus: EventBus;
  private port: number;
  private unsubscribers: (() => void)[] = [];

  constructor(eventBus: EventBus, port: number = 8765) {
    this.eventBus = eventBus;
    this.port = port;
  }

  start(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.wss = new WebSocketServer({ port: this.port });

        this.wss.on('listening', () => {
          logger.info(`Monitor server listening on port ${this.port}`);
          this.setupEventForwarding();
          resolve();
        });

        this.wss.on('connection', (ws: WebSocket) => {
          this.clients.add(ws);
          logger.info(`Monitor client connected (total: ${this.clients.size})`);

          ws.on('message', (data: Buffer) => {
            try {
              const message = JSON.parse(data.toString()) as SubscribeMessage;
              if (message.type === 'subscribe') {
                logger.debug(`Client subscribed to: ${message.events.join(', ')}`);
              }
            } catch (error) {
              logger.error('Invalid monitor message:', error);
            }
          });

          ws.on('close', () => {
            this.clients.delete(ws);
            logger.info(`Monitor client disconnected (total: ${this.clients.size})`);
          });

          ws.on('error', (error: Error) => {
            logger.error('Monitor client error:', error);
            this.clients.delete(ws);
          });

          this.sendToClient(ws, {
            type: 'connected',
            timestamp: Date.now(),
            agentId: '',
            data: { message: 'Monitor connected' },
          });
        });

        this.wss.on('error', (error: Error) => {
          logger.error('Monitor server error:', error);
          reject(error);
        });
      } catch (error) {
        reject(error);
      }
    });
  }

  stop(): void {
    for (const unsub of this.unsubscribers) {
      unsub();
    }
    this.unsubscribers = [];

    for (const client of this.clients) {
      client.close();
    }
    this.clients.clear();

    if (this.wss) {
      this.wss.close();
      this.wss = null;
    }

    logger.info('Monitor server stopped');
  }

  broadcast(event: string, data: unknown): void {
    const message: MonitorEvent = {
      type: event,
      timestamp: Date.now(),
      agentId: '',
      data,
    };

    for (const client of this.clients) {
      this.sendToClient(client, message);
    }
  }

  private setupEventForwarding(): void {
    const events: EventKey[] = [
      'agent:start',
      'agent:thinking',
      'agent:tool_call',
      'agent:tool_result',
      'agent:message',
      'agent:error',
      'agent:done',
      'mode:changed',
      'task:added',
      'task:completed',
      'task:failed',
      'metrics:updated',
    ];

    for (const event of events) {
      const unsub = this.eventBus.on(event, (data) => {
        this.broadcast(event, data);
      });
      this.unsubscribers.push(unsub);
    }
  }

  private sendToClient(client: WebSocket, message: MonitorEvent): void {
    if (client.readyState === WebSocket.OPEN) {
      try {
        client.send(JSON.stringify(message));
      } catch (error) {
        logger.error('Failed to send to monitor client:', error);
      }
    }
  }

  getClientCount(): number {
    return this.clients.size;
  }

  isRunning(): boolean {
    return this.wss !== null;
  }
}
