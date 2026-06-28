import { Message, TokenUsage, ToolCall, ToolResult } from './types.js';
import { logger } from './logger.js';

export type AgentEventMap = {
  'agent:start': { agentId: string; task: string };
  'agent:thinking': { agentId: string; reasoning: string };
  'agent:tool_call': { agentId: string; toolCall: ToolCall };
  'agent:tool_result': { agentId: string; toolResult: ToolResult };
  'agent:message': { agentId: string; content: string };
  'agent:error': { agentId: string; error: string };
  'agent:done': { agentId: string; usage?: TokenUsage };
  'session:created': { sessionId: string };
  'session:message': { sessionId: string; message: Message };
  'mode:changed': { agentId: string; mode: string };
  'task:added': { taskId: string; name: string };
  'task:completed': { taskId: string };
  'task:failed': { taskId: string; error: string };
  'metrics:updated': { activeAgents: number; totalTokens: number; totalCost: number };
};

export type EventKey = keyof AgentEventMap;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Handler = (data: any) => void;

const MAX_LISTENERS = 100;

export class EventBus {
  private handlers: Map<string, Set<Handler>> = new Map();
  private onceHandlers: Map<string, Set<Handler>> = new Map();
  private listenerCount: Map<string, number> = new Map();

  on<K extends EventKey>(event: K, handler: (data: AgentEventMap[K]) => void): () => void {
    return this.addHandler(event, handler as Handler, false);
  }

  once<K extends EventKey>(event: K, handler: (data: AgentEventMap[K]) => void): () => void {
    return this.addHandler(event, handler as Handler, true);
  }

  private addHandler(event: string, handler: Handler, once: boolean): () => void {
    const count = this.listenerCount.get(event) || 0;
    if (count >= MAX_LISTENERS) {
      logger.warn(`EventBus: max listeners (${MAX_LISTENERS}) reached for "${event}"`);
    }

    const targetMap = once ? this.onceHandlers : this.handlers;
    if (!targetMap.has(event)) {
      targetMap.set(event, new Set());
    }
    targetMap.get(event)!.add(handler);
    this.listenerCount.set(event, count + 1);

    return () => {
      this.off(event as EventKey, handler);
    };
  }

  off<K extends EventKey>(event: K, handler: Handler): void {
    this.handlers.get(event)?.delete(handler);
    this.onceHandlers.get(event)?.delete(handler);
    const count = this.listenerCount.get(event) || 0;
    if (count > 0) {
      this.listenerCount.set(event, count - 1);
    }
  }

  emit<K extends EventKey>(event: K, data: AgentEventMap[K]): void {
    const handlers = this.handlers.get(event);
    const onceHandlers = this.onceHandlers.get(event);

    if (handlers) {
      for (const handler of handlers) {
        try {
          handler(data);
        } catch (error) {
          logger.error(`EventBus: handler error for "${event}":`, error);
        }
      }
    }

    if (onceHandlers && onceHandlers.size > 0) {
      for (const handler of onceHandlers) {
        try {
          handler(data);
        } catch (error) {
          logger.error(`EventBus: once handler error for "${event}":`, error);
        }
      }
      onceHandlers.clear();
    }
  }

  onAny(handler: (event: string, data: unknown) => void): () => void {
    const unsubscribers: (() => void)[] = [];
    for (const event of this.getAllEvents()) {
      const h: Handler = (data: unknown) => handler(event, data);
      this.addHandler(event, h, false);
      unsubscribers.push(() => this.off(event as EventKey, h));
    }

    return () => {
      for (const unsub of unsubscribers) {
        unsub();
      }
    };
  }

  private getAllEvents(): string[] {
    const events = new Set<string>();
    for (const key of this.handlers.keys()) {
      events.add(key);
    }
    for (const key of this.onceHandlers.keys()) {
      events.add(key);
    }
    return Array.from(events);
  }

  listenerCountOf(event: string): number {
    const regular = this.handlers.get(event)?.size || 0;
    const once = this.onceHandlers.get(event)?.size || 0;
    return regular + once;
  }

  removeAllListeners(event?: string): void {
    if (event) {
      this.handlers.delete(event);
      this.onceHandlers.delete(event);
      this.listenerCount.delete(event);
    } else {
      this.handlers.clear();
      this.onceHandlers.clear();
      this.listenerCount.clear();
    }
  }
}

export const eventBus = new EventBus();
