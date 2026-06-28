import { Message, ChatChunk, TokenUsage, ProviderConfig, ToolDefinition } from '../core/types.js';

export abstract class BaseProvider {
  protected config: ProviderConfig;
  protected tools: ToolDefinition[] = [];

  constructor(config: ProviderConfig) {
    this.config = config;
  }

  setTools(tools: ToolDefinition[]): void {
    this.tools = tools;
  }

  abstract chat(messages: Message[]): AsyncGenerator<ChatChunk>;
  abstract getModel(): string;
  abstract getContextWindow(): number;

  getProviderType(): string {
    return this.config.type;
  }

  protected estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
