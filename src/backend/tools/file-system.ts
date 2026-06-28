import fs from 'fs/promises';
import path from 'path';
import { BaseTool } from './base.js';
import { ToolDefinition, ToolContext } from '../core/types.js';
import { ToolError } from '../core/errors.js';

export class ReadFileTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'read_file',
      description: '读取文件内容',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: '文件路径' },
        },
        required: ['path'],
      },
      riskLevel: 'read_only',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const filePath = args.path as string;
    const absolutePath = path.resolve(context.workingDir, filePath);

    try {
      const content = await fs.readFile(absolutePath, 'utf-8');
      return content;
    } catch (error) {
      throw new ToolError(`Failed to read file: ${(error as Error).message}`, error as Error);
    }
  }
}

export class WriteFileTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'write_file',
      description: '写入文件内容',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: '文件路径' },
          content: { type: 'string', description: '文件内容' },
        },
        required: ['path', 'content'],
      },
      riskLevel: 'write',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const filePath = args.path as string;
    const content = args.content as string;
    const absolutePath = path.resolve(context.workingDir, filePath);

    try {
      const dir = path.dirname(absolutePath);
      await fs.mkdir(dir, { recursive: true });
      await fs.writeFile(absolutePath, content, 'utf-8');
      return `File written successfully: ${filePath}`;
    } catch (error) {
      throw new ToolError(`Failed to write file: ${(error as Error).message}`, error as Error);
    }
  }
}

export class ListFilesTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'list_files',
      description: '列出目录下的文件',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: '目录路径' },
        },
        required: ['path'],
      },
      riskLevel: 'read_only',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const dirPath = args.path as string;
    const absolutePath = path.resolve(context.workingDir, dirPath);

    try {
      const entries = await fs.readdir(absolutePath, { withFileTypes: true });
      const result = entries.map((entry) => ({
        name: entry.name,
        type: entry.isDirectory() ? 'directory' : 'file',
      }));

      return JSON.stringify(result, null, 2);
    } catch (error) {
      throw new ToolError(`Failed to list files: ${(error as Error).message}`, error as Error);
    }
  }
}

export class EditFileTool extends BaseTool {
  getDefinition(): ToolDefinition {
    return {
      name: 'edit_file',
      description: '编辑文件内容（查找替换）',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: '文件路径' },
          old_text: { type: 'string', description: '要查找的文本' },
          new_text: { type: 'string', description: '替换后的文本' },
        },
        required: ['path', 'old_text', 'new_text'],
      },
      riskLevel: 'write',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    const filePath = args.path as string;
    const oldText = args.old_text as string;
    const newText = args.new_text as string;
    const absolutePath = path.resolve(context.workingDir, filePath);

    try {
      let content = await fs.readFile(absolutePath, 'utf-8');

      if (!content.includes(oldText)) {
        throw new ToolError('Text not found in file');
      }

      content = content.replace(oldText, newText);
      await fs.writeFile(absolutePath, content, 'utf-8');

      return `File edited successfully: ${filePath}`;
    } catch (error) {
      if (error instanceof ToolError) throw error;
      throw new ToolError(`Failed to edit file: ${(error as Error).message}`, error as Error);
    }
  }
}
