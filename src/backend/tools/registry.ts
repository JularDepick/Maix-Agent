import { BaseTool } from './base.js';
import { ToolContext, ToolDefinition } from '../core/types.js';
import { ToolError } from '../core/errors.js';
import { logger } from '../core/logger.js';
import { ReadFileTool, WriteFileTool, ListFilesTool, EditFileTool } from './file-system.js';
import { ExecuteCommandTool } from './shell.js';
import { GrepTool, GlobTool } from './search.js';

export interface ToolApprovalHandler {
  requestApproval(toolName: string, args: Record<string, unknown>): Promise<boolean>;
}

export class ToolRegistry {
  private tools: Map<string, BaseTool> = new Map();
  private autoApprovedTools: Set<string> = new Set();
  private approvalHandler: ToolApprovalHandler | null = null;

  constructor() {
    this.registerDefaultTools();
  }

  private registerDefaultTools(): void {
    this.register(new ReadFileTool());
    this.register(new WriteFileTool());
    this.register(new ListFilesTool());
    this.register(new EditFileTool());
    this.register(new ExecuteCommandTool());
    this.register(new GrepTool());
    this.register(new GlobTool());

    this.autoApprovedTools.add('read_file');
    this.autoApprovedTools.add('list_files');
    this.autoApprovedTools.add('grep');
    this.autoApprovedTools.add('glob');
  }

  setApprovalHandler(handler: ToolApprovalHandler): void {
    this.approvalHandler = handler;
  }

  register(tool: BaseTool): void {
    this.tools.set(tool.getName(), tool);
    logger.debug(`Registered tool: ${tool.getName()}`);
  }

  remove(name: string): boolean {
    const removed = this.tools.delete(name);
    if (removed) {
      this.autoApprovedTools.delete(name);
      logger.debug(`Removed tool: ${name}`);
    }
    return removed;
  }

  get(name: string): BaseTool | undefined {
    return this.tools.get(name);
  }

  list(): ToolDefinition[] {
    return Array.from(this.tools.values()).map((t) => t.getDefinition());
  }

  getToolDefinitions(): ToolDefinition[] {
    return this.list();
  }

  async execute(name: string, args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const tool = this.tools.get(name);
    if (!tool) {
      throw new ToolError(`Tool not found: ${name}`);
    }

    if (!this.autoApprovedTools.has(name)) {
      if (this.approvalHandler) {
        const approved = await this.approvalHandler.requestApproval(name, args);
        if (!approved) {
          throw new ToolError(`Tool ${name} was denied by user`);
        }
      } else {
        throw new ToolError(`Tool ${name} requires approval but no handler is configured`);
      }
    }

    logger.debug(`Executing tool: ${name}`, args);

    const startTime = Date.now();
    try {
      const result = await tool.execute(args, context);
      const duration = Date.now() - startTime;

      if (duration > 10000) {
        logger.warn(`Slow tool execution: ${name} took ${duration}ms`);
      }

      return result;
    } catch (error) {
      logger.error(`Tool execution failed: ${name}`, error);
      throw error;
    }
  }

  autoApproveTool(name: string): void {
    this.autoApprovedTools.add(name);
    logger.info(`Tool auto-approved: ${name}`);
  }

  revokeAutoApproval(name: string): void {
    this.autoApprovedTools.delete(name);
    logger.info(`Tool auto-approval revoked: ${name}`);
  }

  isAutoApproved(name: string): boolean {
    return this.autoApprovedTools.has(name);
  }

  getToolNames(): string[] {
    return Array.from(this.tools.keys());
  }
}
