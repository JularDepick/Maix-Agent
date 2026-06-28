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

export class HttpAdapter implements BackendAPI {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  async *run(input: string): AsyncGenerator<AgentEvent, void, unknown> {
    const response = await fetch(`${this.baseUrl}/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ input })
    });

    if (!response.ok) {
      throw new Error(`HTTP error: ${response.status}`);
    }

    const reader = response.body?.getReader();
    if (!reader) throw new Error('No response body');

    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          try {
            const event = JSON.parse(line.slice(6));
            yield event as AgentEvent;
          } catch (e) {
            // ignore parse errors
          }
        }
      }
    }
  }

  getSessionManager(): SessionManagerAPI {
    return {
      getCurrentSession: async () => {
        const res = await fetch(`${this.baseUrl}/api/sessions/current`);
        return res.ok ? (await res.json()) as Session : null;
      },
      listSessions: async () => {
        const res = await fetch(`${this.baseUrl}/api/sessions`);
        return res.ok ? (await res.json()) as Session[] : [];
      },
      createSession: async (name?: string) => {
        const res = await fetch(`${this.baseUrl}/api/sessions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name })
        });
        return (await res.json()) as Session;
      },
      clearMessages: async () => {
        await fetch(`${this.baseUrl}/api/sessions/current/messages`, {
          method: 'DELETE'
        });
      }
    };
  }

  getProvider(): ProviderAPI {
    return {
      getModel: async () => {
        const res = await fetch(`${this.baseUrl}/api/provider/model`);
        return res.ok ? ((await res.json()) as { model: string }).model : 'unknown';
      }
    };
  }

  getTools(): ToolsAPI | null {
    return {
      autoApproveTool: async (name: string) => {
        await fetch(`${this.baseUrl}/api/tools/auto-approve`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name })
        });
      }
    };
  }

  getModeManager(): ModeManagerAPI {
    return {
      getCurrentMode: async () => {
        const res = await fetch(`${this.baseUrl}/api/mode/current`);
        return res.ok ? ((await res.json()) as { mode: AgentMode }).mode : 'agent';
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
        await fetch(`${this.baseUrl}/api/mode/switch`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ mode })
        });
      }
    };
  }

  getId(): string {
    return 'remote-backend';
  }

  getMemory(): MemoryAPI {
    return {
      saveSession: async (session: Session) => {
        await fetch(`${this.baseUrl}/api/sessions/${session.id}/save`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(session)
        });
      }
    };
  }
}