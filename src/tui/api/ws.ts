import {
  BackendAPI,
  SessionManagerAPI,
  ProviderAPI,
  ToolsAPI,
  ModeManagerAPI,
  MemoryAPI,
  Session,
  AgentEvent,
  AgentMode
} from './types.js';

export class WsAdapter implements BackendAPI {
  private ws: WebSocket | null = null;
  private url: string;
  private eventQueue: AgentEvent[] = [];
  private waitResolve: (() => void) | null = null;

  constructor(url: string) {
    this.url = url;
  }

  private connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        resolve();
        return;
      }

      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => resolve();
      this.ws.onerror = (e) => reject(e);
      this.ws.onmessage = (e) => {
        try {
          const event = JSON.parse(e.data as string) as AgentEvent;
          this.eventQueue.push(event);
          this.waitResolve?.();
        } catch (err) {
          // ignore
        }
      };
    });
  }

  private async waitForEvent(): Promise<AgentEvent | null> {
    while (this.eventQueue.length === 0) {
      await new Promise<void>((resolve) => {
        this.waitResolve = resolve;
      });
    }
    return this.eventQueue.shift() || null;
  }

  async *run(input: string): AsyncGenerator<AgentEvent, void, unknown> {
    await this.connect();

    this.ws!.send(JSON.stringify({ type: 'chat', input }));

    while (true) {
      const event = await this.waitForEvent();
      if (!event) break;
      yield event;
      if (event.type === 'done') break;
    }
  }

  getSessionManager(): SessionManagerAPI {
    return {
      getCurrentSession: async () => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'get_current_session' }));
        const event = await this.waitForEvent();
        return event?.content ? JSON.parse(event.content) : null;
      },
      listSessions: async () => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'list_sessions' }));
        const event = await this.waitForEvent();
        return event?.content ? JSON.parse(event.content) : [];
      },
      createSession: async (name?: string) => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'create_session', name }));
        const event = await this.waitForEvent();
        return event?.content ? JSON.parse(event.content) : ({} as Session);
      },
      clearMessages: async () => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'clear_messages' }));
        await this.waitForEvent();
      }
    };
  }

  getProvider(): ProviderAPI {
    return {
      getModel: async () => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'get_model' }));
        const event = await this.waitForEvent();
        return event?.content || 'unknown';
      }
    };
  }

  getTools(): ToolsAPI | null {
    return {
      autoApproveTool: async (name: string) => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'auto_approve_tool', name }));
        await this.waitForEvent();
      }
    };
  }

  getModeManager(): ModeManagerAPI {
    return {
      getCurrentMode: async () => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'get_mode' }));
        const event = await this.waitForEvent();
        return (event?.content as AgentMode) || 'agent';
      },
      getAvailableModes: async () => ['plan', 'agent', 'yolo'],
      getModeDescription: (mode: AgentMode) => {
        const descriptions: Record<AgentMode, string> = {
          plan: 'Plan mode - Agent creates and follows a plan',
          agent: 'Agent mode - Agent autonomously completes tasks',
          yolo: 'YOLO mode - Auto-approve all tool calls'
        };
        return descriptions[mode];
      },
      switchMode: async (mode: AgentMode) => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'switch_mode', mode }));
        await this.waitForEvent();
      }
    };
  }

  getId(): string {
    return 'ws-backend';
  }

  getMemory(): MemoryAPI {
    return {
      saveSession: async (session: Session) => {
        await this.connect();
        this.ws!.send(JSON.stringify({ type: 'save_session', session }));
        await this.waitForEvent();
      }
    };
  }
}