import { Message } from '../core/types.js';

export class ContextManager {
  private maxTokens: number;
  private compactionThreshold: number;

  constructor(maxTokens: number = 128000) {
    this.maxTokens = maxTokens;
    this.compactionThreshold = maxTokens * 0.8;
  }

  estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }

  calculateMessageTokens(messages: Message[]): number {
    let total = 0;
    for (const msg of messages) {
      total += this.estimateTokens(msg.content);
      if (msg.toolCalls) {
        for (const tc of msg.toolCalls) {
          total += this.estimateTokens(JSON.stringify(tc.arguments));
        }
      }
      if (msg.toolResult) {
        total += this.estimateTokens(msg.toolResult.result);
      }
    }
    return total;
  }

  needsCompaction(messages: Message[]): boolean {
    return this.calculateMessageTokens(messages) > this.compactionThreshold;
  }

  compactMessages(messages: Message[], targetTokens: number): Message[] {
    if (messages.length <= 2) return messages;

    const systemMessages = messages.filter(m => m.role === 'system');
    const nonSystemMessages = messages.filter(m => m.role !== 'system');

    const systemTokens = this.calculateMessageTokens(systemMessages);
    const availableTokens = targetTokens - systemTokens;

    let currentTokens = 0;
    const keptMessages: Message[] = [];

    for (let i = nonSystemMessages.length - 1; i >= 0; i--) {
      const msg = nonSystemMessages[i];
      const msgTokens = this.calculateMessageTokens([msg]);

      if (currentTokens + msgTokens > availableTokens) {
        const summary = this.createSummary(nonSystemMessages.slice(0, i + 1));
        keptMessages.unshift({
          id: 'compacted',
          role: 'system',
          content: `[Previous conversation summary]\n${summary}`,
          timestamp: Date.now(),
        });
        break;
      }

      keptMessages.unshift(msg);
      currentTokens += msgTokens;
    }

    return [...systemMessages, ...keptMessages];
  }

  private createSummary(messages: Message[]): string {
    const userMessages = messages.filter(m => m.role === 'user');
    const assistantMessages = messages.filter(m => m.role === 'assistant');

    const parts: string[] = [];

    if (userMessages.length > 0) {
      parts.push(`User discussed: ${userMessages.map(m => m.content.slice(0, 100)).join('; ')}`);
    }

    if (assistantMessages.length > 0) {
      parts.push(`Assistant provided: ${assistantMessages.map(m => m.content.slice(0, 100)).join('; ')}`);
    }

    return parts.join('\n');
  }

  truncateResponse(content: string, maxTokens: number): string {
    const maxChars = maxTokens * 4;
    if (content.length <= maxChars) return content;
    return content.slice(0, maxChars) + '\n... [truncated]';
  }
}
