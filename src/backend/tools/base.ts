import { ToolDefinition, ToolContext } from '../core/types.js';

export abstract class BaseTool {
  abstract getDefinition(): ToolDefinition;
  abstract execute(args: Record<string, unknown>, context: ToolContext): Promise<string>;

  getName(): string {
    return this.getDefinition().name;
  }

  getDescription(): string {
    return this.getDefinition().description;
  }

  getRiskLevel(): string {
    return this.getDefinition().riskLevel;
  }
}
