import Anthropic from '@anthropic-ai/sdk';
import { BaseProvider } from './base.js';
import { Message, ChatChunk, ProviderConfig, ToolDefinition } from '../core/types.js';
import { ProviderError } from '../core/errors.js';
import { logger } from '../core/logger.js';

export class AnthropicProvider extends BaseProvider {
  private client: Anthropic;

  constructor(config: ProviderConfig) {
    super(config);
    this.client = new Anthropic({
      apiKey: config.apiKey,
    });
  }

  async *chat(messages: Message[]): AsyncGenerator<ChatChunk> {
    try {
      const { system, formattedMessages } = this.formatMessages(messages);
      const tools = this.formatTools();

      logger.debug(`Calling Anthropic API with model: ${this.config.model}`);

      const stream = this.client.messages.stream({
        model: this.config.model,
        max_tokens: 4096,
        system: system || undefined,
        messages: formattedMessages,
        tools: tools.length > 0 ? tools : undefined,
      });

      let currentToolCall: { id: string; name: string; input: string } | null = null;
      let inputTokens = 0;
      let outputTokens = 0;

      for await (const event of stream) {
        if (event.type === 'content_block_start') {
          if (event.content_block.type === 'tool_use') {
            currentToolCall = {
              id: event.content_block.id,
              name: event.content_block.name,
              input: '',
            };
          }
        }

        if (event.type === 'content_block_delta') {
          if (event.delta.type === 'text_delta') {
            yield { type: 'text', content: event.delta.text };
          }

          if (event.delta.type === 'input_json_delta' && currentToolCall) {
            currentToolCall.input += event.delta.partial_json;
          }
        }

        if (event.type === 'content_block_stop' && currentToolCall) {
          try {
            yield {
              type: 'tool_call',
              toolCall: {
                id: currentToolCall.id,
                name: currentToolCall.name,
                arguments: JSON.parse(currentToolCall.input || '{}'),
              },
            };
          } catch (e) {
            logger.error('Failed to parse tool input:', e);
          }
          currentToolCall = null;
        }

        if (event.type === 'message_delta') {
          outputTokens = event.usage?.output_tokens || outputTokens;
        }

        if (event.type === 'message_start') {
          inputTokens = event.message.usage?.input_tokens || inputTokens;
        }

        if (event.type === 'message_stop') {
          yield {
            type: 'done',
            usage: {
              promptTokens: inputTokens,
              completionTokens: outputTokens,
              totalTokens: inputTokens + outputTokens,
            },
          };
        }
      }
    } catch (error) {
      logger.error('Anthropic API error:', error);
      throw new ProviderError(`Anthropic API error: ${(error as Error).message}`, error as Error);
    }
  }

  private formatMessages(messages: Message[]): {
    system: string;
    formattedMessages: Anthropic.MessageParam[];
  } {
    let system = '';
    const formattedMessages: Anthropic.MessageParam[] = [];

    for (const msg of messages) {
      if (msg.role === 'system') {
        system = msg.content;
        continue;
      }

      if (msg.role === 'tool') {
        formattedMessages.push({
          role: 'user',
          content: [
            {
              type: 'tool_result',
              tool_use_id: msg.toolResult?.toolCallId || '',
              content: msg.content,
            },
          ],
        });
        continue;
      }

      if (msg.toolCalls && msg.toolCalls.length > 0) {
        formattedMessages.push({
          role: 'assistant',
          content: [
            ...(msg.content ? [{ type: 'text' as const, text: msg.content }] : []),
            ...msg.toolCalls.map((tc) => ({
              type: 'tool_use' as const,
              id: tc.id,
              name: tc.name,
              input: tc.arguments,
            })),
          ],
        });
        continue;
      }

      formattedMessages.push({
        role: msg.role as 'user' | 'assistant',
        content: msg.content,
      });
    }

    return { system, formattedMessages };
  }

  private formatTools(): Anthropic.Tool[] {
    return this.tools.map((tool) => ({
      name: tool.name,
      description: tool.description,
      input_schema: tool.parameters as Anthropic.Tool['input_schema'],
    }));
  }

  getModel(): string {
    return this.config.model;
  }

  getContextWindow(): number {
    const model = this.config.model;
    if (model.includes('claude-3.5') || model.includes('claude-sonnet')) return 200000;
    if (model.includes('claude-3')) return 200000;
    return 100000;
  }
}
