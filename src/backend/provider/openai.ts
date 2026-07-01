import OpenAI from 'openai';
import { BaseProvider } from './base.js';
import { Message, ChatChunk, ProviderConfig, ToolDefinition } from '../core/types.js';
import { ProviderError } from '../core/errors.js';
import { logger } from '../core/logger.js';

export class OpenAIProvider extends BaseProvider {
  private client: OpenAI;

  constructor(config: ProviderConfig) {
    super(config);
    this.client = new OpenAI({
      apiKey: config.apiKey,
      baseURL: config.baseUrl,
    });
  }

  async *chat(messages: Message[]): AsyncGenerator<ChatChunk> {
    try {
      const formattedMessages = this.formatMessages(messages);
      const tools = this.formatTools();

      logger.debug(`Calling OpenAI API with model: ${this.config.model}`);

      const stream = await this.client.chat.completions.create({
        model: this.config.model,
        messages: formattedMessages,
        stream: true,
        tools: tools.length > 0 ? tools : undefined,
      });

      let currentToolCall: { id: string; name: string; arguments: string } | null = null;

      for await (const chunk of stream) {
        const delta = chunk.choices[0]?.delta;

        if (!delta) continue;

        if (delta.content) {
          yield { type: 'text', content: delta.content };
        }

        if (delta.tool_calls) {
          for (const toolCall of delta.tool_calls) {
            if (toolCall.index !== undefined) {
              if (toolCall.id && toolCall.function?.name) {
                if (currentToolCall) {
                  try {
                    yield {
                      type: 'tool_call',
                      toolCall: {
                        id: currentToolCall.id,
                        name: currentToolCall.name,
                        arguments: JSON.parse(currentToolCall.arguments || '{}'),
                      },
                    };
                  } catch (e) {
                    logger.error('Failed to parse tool arguments:', e);
                  }
                }
                currentToolCall = {
                  id: toolCall.id,
                  name: toolCall.function.name,
                  arguments: toolCall.function.arguments || '',
                };
              } else if (toolCall.function?.arguments != null && currentToolCall) {
                currentToolCall.arguments += toolCall.function.arguments;
              }
            }
          }
        }

        if (chunk.choices[0]?.finish_reason === 'tool_calls' && currentToolCall) {
          try {
            yield {
              type: 'tool_call',
              toolCall: {
                id: currentToolCall.id,
                name: currentToolCall.name,
                arguments: JSON.parse(currentToolCall.arguments || '{}'),
              },
            };
          } catch (e) {
            logger.error('Failed to parse tool arguments:', e);
          }
          currentToolCall = null;
        }

        if (chunk.choices[0]?.finish_reason === 'stop') {
          const usage = chunk.usage;
          if (usage) {
            yield {
              type: 'done',
              usage: {
                promptTokens: usage.prompt_tokens,
                completionTokens: usage.completion_tokens,
                totalTokens: usage.total_tokens,
              },
            };
          } else {
            yield { type: 'done' };
          }
        }
      }
    } catch (error) {
      logger.error('OpenAI API error:', error);
      throw new ProviderError(`OpenAI API error: ${(error as Error).message}`, error as Error);
    }
  }

  private formatMessages(messages: Message[]): OpenAI.Chat.ChatCompletionMessageParam[] {
    return messages.map((msg) => {
      if (msg.role === 'tool') {
        return {
          role: 'tool' as const,
          tool_call_id: msg.toolResult?.toolCallId || '',
          content: msg.content,
        };
      }

      if (msg.toolCalls && msg.toolCalls.length > 0) {
        return {
          role: 'assistant' as const,
          content: msg.content || null,
          tool_calls: msg.toolCalls.map((tc) => ({
            id: tc.id,
            type: 'function' as const,
            function: {
              name: tc.name,
              arguments: JSON.stringify(tc.arguments),
            },
          })),
        };
      }

      return {
        role: msg.role as 'user' | 'assistant' | 'system',
        content: msg.content,
      };
    });
  }

  private formatTools(): OpenAI.Chat.ChatCompletionTool[] {
    return this.tools.map((tool) => ({
      type: 'function' as const,
      function: {
        name: tool.name,
        description: tool.description,
        parameters: tool.parameters,
      },
    }));
  }

  getModel(): string {
    return this.config.model;
  }

  getContextWindow(): number {
    const model = this.config.model;
    if (model.includes('gpt-4o')) return 128000;
    if (model.includes('gpt-4')) return 128000;
    if (model.includes('gpt-3.5')) return 16000;
    return 128000;
  }
}
