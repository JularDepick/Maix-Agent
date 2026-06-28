export type Role = 'user' | 'assistant' | 'system' | 'tool';

export interface Message {
  id: string;
  role: Role;
  content: string;
  timestamp: number;
  toolCalls?: ToolCall[];
  toolResult?: ToolResult;
  reasoning?: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolResult {
  toolCallId: string;
  result: string;
  error?: string;
  duration?: number;
}

export interface Session {
  id: string;
  name: string;
  messages: Message[];
  createdAt: number;
  updatedAt: number;
  totalTokens: number;
  totalCost: number;
}

export type AgentEventType =
  | 'text' | 'reasoning' | 'tool_call' | 'tool_approval' | 'tool_result'
  | 'error' | 'done' | 'mode_changed';

export interface AgentEvent {
  type: AgentEventType;
  content?: string;
  toolCall?: ToolCall;
  toolResult?: ToolResult;
  error?: string;
  approved?: boolean;
  mode?: string;
}

export type AgentMode = 'plan' | 'agent' | 'yolo';

export interface BackendAPI {
  run(input: string): AsyncGenerator<AgentEvent, void, unknown>;
  getSessionManager(): SessionManagerAPI;
  getProvider(): ProviderAPI;
  getTools(): ToolsAPI | null;
  getModeManager(): ModeManagerAPI;
  getId(): string;
  getMemory(): MemoryAPI;
}

export interface SessionManagerAPI {
  getCurrentSession(): Promise<Session | null>;
  listSessions(): Promise<Session[]>;
  createSession(name?: string): Promise<Session>;
  clearMessages(): Promise<void>;
}

export interface ProviderAPI {
  getModel(): Promise<string>;
}

export interface ToolsAPI {
  autoApproveTool(name: string): Promise<void>;
}

export interface ModeManagerAPI {
  getCurrentMode(): Promise<AgentMode>;
  getAvailableModes(): Promise<AgentMode[]>;
  getModeDescription(mode: AgentMode): string;
  switchMode(mode: AgentMode): Promise<void>;
}

export interface MemoryAPI {
  saveSession(session: Session): Promise<void>;
}