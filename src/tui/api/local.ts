import { Agent } from '../../backend/agent/agent.js';
import { SessionManager } from '../../backend/agent/session.js';
import { ModeManager, AgentMode } from '../../backend/agent/modes.js';
import {
  BackendAPI,
  SessionManagerAPI,
  ProviderAPI,
  ToolsAPI,
  ModeManagerAPI,
  MemoryAPI,
  Session,
  AgentEvent
} from './types.js';
import { AgentEvent as BackendAgentEvent } from '../../backend/core/types.js';

export class LocalAdapter implements BackendAPI {
  private agent: Agent;

  constructor(agent: Agent) {
    this.agent = agent;
  }

  async *run(input: string): AsyncGenerator<AgentEvent, void, unknown> {
    const generator = this.agent.run(input);
    let result = generator.next();

    while (!(await result).done) {
      const event = (await result).value as BackendAgentEvent;
      yield this.mapEvent(event);
      result = generator.next();
    }
  }

  private mapEvent(event: BackendAgentEvent): AgentEvent {
    return {
      type: event.type as AgentEvent['type'],
      content: event.content,
      toolCall: event.toolCall,
      toolResult: event.toolResult,
      error: event.error,
      approved: event.approved,
      mode: event.mode
    };
  }

  getSessionManager(): SessionManagerAPI {
    const sm = this.agent.getSessionManager();
    return {
      getCurrentSession: async () => sm.getCurrentSession() as Session | null,
      listSessions: async () => sm.listSessions() as Session[],
      createSession: async (name?: string) => sm.createSession(name) as Session,
      clearMessages: async () => sm.clearMessages()
    };
  }

  getProvider(): ProviderAPI {
    const p = this.agent.getProvider();
    return {
      getModel: async () => p.getModel()
    };
  }

  getTools(): ToolsAPI | null {
    const t = this.agent.getTools();
    if (!t) return null;
    return {
      autoApproveTool: async (name: string) => t.autoApproveTool(name)
    };
  }

  getModeManager(): ModeManagerAPI {
    const mm = new ModeManager();
    return {
      getCurrentMode: async () => mm.getCurrentMode() as AgentMode,
      getAvailableModes: async () => mm.getAvailableModes() as AgentMode[],
      getModeDescription: (mode: AgentMode) => mm.getModeDescription(mode),
      switchMode: async (mode: AgentMode) => mm.switchMode(mode)
    };
  }

  getId(): string {
    return this.agent.getId();
  }

  getMemory(): MemoryAPI {
    const m = this.agent.getMemory();
    return {
      saveSession: async (session: Session) => m.saveSession(session as any)
    };
  }
}