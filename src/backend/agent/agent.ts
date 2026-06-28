import { v4 as uuidv4 } from 'uuid';
import { BaseProvider } from '../provider/base.js';
import { SessionManager } from './session.js';
import { MemoryStore } from './memory.js';
import { ContextManager } from './context.js';
import { ModeManager } from './modes.js';
import { Message, AgentEvent, ToolCall, ToolResult, TokenUsage } from '../core/types.js';
import { logger } from '../core/logger.js';
import { ToolRegistry } from '../tools/registry.js';

const MAX_TOOL_ROUNDS = 16;

export interface AgentConfig {
  provider: BaseProvider;
  memory: MemoryStore;
  tools: ToolRegistry;
  systemPrompt?: string;
  workingDir: string;
  maxContextTokens?: number;
}

export class Agent {
  private id: string = uuidv4();
  private provider: BaseProvider;
  private memory: MemoryStore;
  private tools: ToolRegistry;
  private context: ContextManager;
  private modeManager: ModeManager;
  private systemPrompt: string;
  private workingDir: string;
  private sessions: SessionManager;

  constructor(config: AgentConfig) {
    this.provider = config.provider;
    this.memory = config.memory;
    this.tools = config.tools;
    this.workingDir = config.workingDir;
    this.systemPrompt = config.systemPrompt || this.getDefaultSystemPrompt();
    this.sessions = new SessionManager();
    this.context = new ContextManager(config.maxContextTokens || 128000);
    this.modeManager = new ModeManager();
  }

  getId(): string {
    return this.id;
  }

  setId(id: string): void {
    this.id = id;
  }

  async init(): Promise<void> {
    this.sessions.loadSessions(this.memory);
    const count = this.sessions.listSessions().length;
    if (count > 0) {
      logger.info(`Agent initialized with ${count} persisted session(s)`);
    }
  }

  private getDefaultSystemPrompt(): string {
    return `You are Maix, an AI coding assistant. You can help with:
- Reading and writing files
- Executing commands
- Searching code
- Answering questions

Always be helpful, concise, and accurate. When working with files, use the provided tools.`;
  }

  async *run(userInput: string): AsyncGenerator<AgentEvent, void, AgentEvent | undefined> {
    let session = this.sessions.getCurrentSession();
    if (!session) {
      session = this.sessions.createSession();
    }

    this.sessions.addMessage('user', userInput);

    let contextMessages = this.buildContext();

    if (this.context.needsCompaction(contextMessages)) {
      contextMessages = this.context.compactMessages(contextMessages, this.provider.getContextWindow() * 0.7);
      yield { type: 'text', content: '[Context compacted]\n' };
    }

    let finalUsage: TokenUsage | undefined;

    for (let round = 0; round < MAX_TOOL_ROUNDS; round++) {
      try {
        const stream = this.provider.chat(contextMessages);

        let currentContent = '';
        let currentToolCalls: ToolCall[] = [];

        for await (const chunk of stream) {
          if (chunk.type === 'text') {
            currentContent += chunk.content;
            yield { type: 'text', content: chunk.content };
          }

          if (chunk.type === 'reasoning' && chunk.content) {
            yield { type: 'reasoning', content: chunk.content };
          }

          if (chunk.type === 'tool_call' && chunk.toolCall) {
            currentToolCalls.push(chunk.toolCall);
          }

          if (chunk.type === 'done' && chunk.usage) {
            finalUsage = chunk.usage;
          }
        }

        if (finalUsage) {
          const cost = this.calculateCost(finalUsage);
          this.sessions.updateTokenUsage(finalUsage.promptTokens, finalUsage.completionTokens, cost);
        }

        if (currentContent || currentToolCalls.length > 0) {
          this.sessions.addMessage('assistant', currentContent, {
            toolCalls: currentToolCalls.length > 0 ? currentToolCalls : undefined,
          });
        }

        if (currentToolCalls.length > 0) {
          for (const toolCall of currentToolCalls) {
            /* 向 TUI 发送工具调用审批请求 */
            yield {
              type: 'tool_approval',
              toolCall,
              approved: false,
            };

            /* 检查工具是否已自动批准 */
            const autoApproved = this.tools.isAutoApproved(toolCall.name);

            if (!autoApproved) {
              /* 等待用户决策，由 TUI 通过 generator.next() 传入 */
              /* 这里默认拒绝，实际审批由 TUI 的 requestToolApproval 处理 */
              const toolResult: ToolResult = {
                toolCallId: toolCall.id,
                result: '',
                error: 'Tool requires approval',
              };
              this.sessions.addMessage('tool', 'Tool requires approval', { toolResult });
              yield { type: 'tool_result', toolResult };
              continue;
            }

            /* 自动批准，继续执行 */
            yield { type: 'tool_call', toolCall };

            try {
              const startTime = Date.now();
              const result = await this.tools.execute(toolCall.name, toolCall.arguments, {
                workingDir: this.workingDir,
                sessionId: this.sessions.getCurrentSession()?.id || '',
              });
              const duration = Date.now() - startTime;

              const toolResult: ToolResult = {
                toolCallId: toolCall.id,
                result,
                duration,
              };

              this.sessions.addMessage('tool', result, { toolResult });
              yield { type: 'tool_result', toolResult };
            } catch (error) {
              const toolResult: ToolResult = {
                toolCallId: toolCall.id,
                result: '',
                error: (error as Error).message,
              };

              this.sessions.addMessage('tool', `Error: ${(error as Error).message}`, { toolResult });
              yield { type: 'tool_result', toolResult };
            }
          }

          contextMessages = this.buildContext();
        } else {
          break;
        }
      } catch (error) {
        logger.error('Agent error:', error);
        yield { type: 'error', error: (error as Error).message };
        break;
      }
    }

    this.saveMemory();
    this.persistSession();
    yield { type: 'done', usage: finalUsage };
  }

  private buildContext(): Message[] {
    const messages: Message[] = [];

    messages.push({
      id: 'system',
      role: 'system',
      content: this.systemPrompt,
      timestamp: 0,
    });

    const memoryContext = this.memory.getContextForSession(
      this.sessions.getCurrentSession()?.id || '',
      2000
    );

    if (memoryContext) {
      messages.push({
        id: 'memory',
        role: 'system',
        content: `Relevant context from previous sessions:\n${memoryContext}`,
        timestamp: 0,
      });
    }

    messages.push(...this.sessions.getMessages());

    return messages;
  }

  private saveMemory(): void {
    const session = this.sessions.getCurrentSession();
    if (!session) return;

    const userMessages = session.messages.filter((m) => m.role === 'user');
    const assistantMessages = session.messages.filter((m) => m.role === 'assistant');

    if (userMessages.length > 0) {
      const lastUserMsg = userMessages[userMessages.length - 1];
      this.memory.saveMemory({
        sessionId: session.id,
        type: 'episodic',
        content: `User asked: ${lastUserMsg.content}`,
        importance: 0.6,
      });
    }

    if (assistantMessages.length > 0) {
      const lastAssistantMsg = assistantMessages[assistantMessages.length - 1];
      this.memory.saveMemory({
        sessionId: session.id,
        type: 'episodic',
        content: `Assistant responded: ${lastAssistantMsg.content.slice(0, 200)}`,
        importance: 0.5,
      });
    }
  }

  private persistSession(): void {
    const session = this.sessions.getCurrentSession();
    if (session) {
      this.memory.saveSession(session);
    }
  }

  private calculateCost(usage: TokenUsage): number {
    const { promptTokens, completionTokens } = usage;
    const model = this.provider.getModel();

    if (model.includes('gpt-4o')) {
      return (promptTokens * 0.005 + completionTokens * 0.015) / 1000;
    }
    if (model.includes('gpt-4')) {
      return (promptTokens * 0.03 + completionTokens * 0.06) / 1000;
    }
    if (model.includes('claude-3.5') || model.includes('claude-sonnet')) {
      return (promptTokens * 0.003 + completionTokens * 0.015) / 1000;
    }
    if (model.includes('claude-3')) {
      return (promptTokens * 0.015 + completionTokens * 0.075) / 1000;
    }
    return (promptTokens * 0.001 + completionTokens * 0.002) / 1000;
  }

  getSessionManager(): SessionManager {
    return this.sessions;
  }

  getProvider(): BaseProvider {
    return this.provider;
  }

  getMemory(): MemoryStore {
    return this.memory;
  }

  getTools(): ToolRegistry {
    return this.tools;
  }

  getModeManager(): ModeManager {
    return this.modeManager;
  }

  switchProvider(provider: BaseProvider): void {
    this.provider = provider;
    logger.info(`Switched to provider: ${provider.getModel()}`);
  }
}
