import fs from 'fs/promises';
import path from 'path';
import { BaseTool } from './base.js';
import { ToolDefinition, ToolContext } from '../core/types.js';
import { ToolError } from '../core/errors.js';

export class GrepTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'grep',
      description: '搜索文件内容',
      parameters: {
        type: 'object',
        properties: {
          pattern: { type: 'string', description: '搜索模式（正则表达式）' },
          path: { type: 'string', description: '搜索路径' },
          include: { type: 'string', description: '文件过滤（如 *.ts）' },
        },
        required: ['pattern'],
      },
      riskLevel: 'read_only',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const pattern = args.pattern as string;
    const searchPath = (args.path as string) || '.';
    const include = args.include as string;
    const absolutePath = path.resolve(context.workingDir, searchPath);

    try {
      const results: string[] = [];
      await this.searchDir(absolutePath, new RegExp(pattern), include, results, 0);

      if (results.length === 0) {
        return 'No matches found';
      }

      return results.slice(0, 100).join('\n');
    } catch (error) {
      throw new ToolError(`Grep failed: ${(error as Error).message}`, error as Error);
    }
  }

  private async searchDir(
    dir: string,
    pattern: RegExp,
    include: string | undefined,
    results: string[],
    depth: number
  ): Promise<void> {
    if (depth > 10) return;

    try {
      const entries = await fs.readdir(dir, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);

        if (entry.isDirectory()) {
          if (!entry.name.startsWith('.') && entry.name !== 'node_modules') {
            await this.searchDir(fullPath, pattern, include, results, depth + 1);
          }
        } else {
          if (include && !this.matchFilter(entry.name, include)) continue;

          try {
            const content = await fs.readFile(fullPath, 'utf-8');
            const lines = content.split('\n');

            for (let i = 0; i < lines.length; i++) {
              if (pattern.test(lines[i])) {
                results.push(`${fullPath}:${i + 1}: ${lines[i].trim()}`);
              }
            }
          } catch {
            // Skip binary files
          }
        }
      }
    } catch {
      // Skip inaccessible directories
    }
  }

  private matchFilter(filename: string, filter: string): boolean {
    const escaped = filter.replace(/[.+^${}()|[\]\\]/g, '\\$&');
    const regex = escaped.replace(/\*/g, '.*').replace(/\?/g, '.');
    return new RegExp(`^${regex}$`).test(filename);
  }
}

export class GlobTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'glob',
      description: '查找匹配的文件',
      parameters: {
        type: 'object',
        properties: {
          pattern: { type: 'string', description: 'glob 模式' },
          path: { type: 'string', description: '搜索路径' },
        },
        required: ['pattern'],
      },
      riskLevel: 'read_only',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const pattern = args.pattern as string;
    const searchPath = (args.path as string) || '.';
    const absolutePath = path.resolve(context.workingDir, searchPath);

    try {
      const results: string[] = [];
      await this.findFiles(absolutePath, pattern, results, 0);

      if (results.length === 0) {
        return 'No files found';
      }

      return results.slice(0, 100).join('\n');
    } catch (error) {
      throw new ToolError(`Glob failed: ${(error as Error).message}`, error as Error);
    }
  }

  private async findFiles(dir: string, pattern: string, results: string[], depth: number): Promise<void> {
    if (depth > 10) return;

    try {
      const entries = await fs.readdir(dir, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        const relativePath = path.relative(process.cwd(), fullPath);

        if (entry.isDirectory()) {
          if (!entry.name.startsWith('.') && entry.name !== 'node_modules') {
            await this.findFiles(fullPath, pattern, results, depth + 1);
          }
        } else {
          if (this.matchGlob(entry.name, pattern)) {
            results.push(relativePath);
          }
        }
      }
    } catch {
      // Skip inaccessible directories
    }
  }

  private matchGlob(filename: string, pattern: string): boolean {
    const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&');
    const regex = escaped
      .replace(/\*/g, '.*')
      .replace(/\?/g, '.');
    return new RegExp(`^${regex}$`).test(filename);
  }
}
