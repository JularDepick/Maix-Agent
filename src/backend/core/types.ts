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

export interface ProviderConfig {
  type: 'openai' | 'anthropic';
  apiKey: string;
  baseUrl?: string;
  model: string;
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

export interface ChatChunk {
  type: 'text' | 'reasoning' | 'tool_call' | 'done';
  content?: string;
  toolCall?: ToolCall;
  usage?: TokenUsage;
}

export interface TokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export type AgentEventType =
  | 'text' | 'reasoning' | 'tool_call' | 'tool_approval' | 'tool_result'
  | 'error' | 'done' | 'mode_changed' | 'plan_created' | 'plan_step';

export interface AgentEvent {
  type: AgentEventType;
  content?: string;
  toolCall?: ToolCall;
  toolResult?: ToolResult;
  error?: string;
  usage?: TokenUsage;
  approved?: boolean;
  mode?: string;
  plan?: PlanStep[];
  stepIndex?: number;
}

export interface PlanStep {
  id: string;
  description: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  toolCalls?: ToolCall[];
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
  riskLevel: 'read_only' | 'write' | 'shell' | 'network';
}

export interface ToolContext {
  workingDir: string;
  sessionId: string;
}

export interface Theme {
  name: string;
  bg: string;
  fg: string;
  accent: string;
  dim: string;
  warn: string;
  error: string;
  success: string;
  border: string;
  userMsg: string;
  assistantMsg: string;
  systemMsg: string;
}
