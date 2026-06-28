import { exec } from 'child_process';
import { promisify } from 'util';
import { BaseTool } from './base.js';
import { ToolDefinition, ToolContext } from '../core/types.js';
import { ToolError } from '../core/errors.js';

const execAsync = promisify(exec);

const DANGEROUS_COMMANDS = [
  'rm -rf /',
  'rm -rf /*',
  'mkfs',
  'dd if=',
  ':(){:|:&};:',
  'chmod -R 777 /',
  'chown -R',
];

export class ExecuteCommandTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'execute_command',
      description: '执行系统命令',
      parameters: {
        type: 'object',
        properties: {
          command: { type: 'string', description: '要执行的命令' },
          timeout: { type: 'number', description: '超时时间（毫秒）' },
        },
        required: ['command'],
      },
      riskLevel: 'shell',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const command = args.command as string;
    const timeout = (args.timeout as number) || 120000;

    for (const dangerous of DANGEROUS_COMMANDS) {
      if (command.includes(dangerous)) {
        throw new ToolError(`Dangerous command blocked: ${command}`);
      }
    }

    try {
      const { stdout, stderr } = await execAsync(command, {
        cwd: context.workingDir,
        timeout,
        maxBuffer: 1024 * 1024 * 10,
      });

      let result = '';
      if (stdout) result += stdout;
      if (stderr) result += `\nSTDERR:\n${stderr}`;

      return result || 'Command executed successfully (no output)';
    } catch (error: unknown) {
      const err = error as { killed?: boolean; message?: string };
      if (err.killed) {
        throw new ToolError(`Command timed out after ${timeout}ms`);
      }
      throw new ToolError(`Command failed: ${err.message || 'Unknown error'}`, error as Error);
    }
  }
}
