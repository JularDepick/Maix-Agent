import fs from 'fs/promises';
import path from 'path';
import { v4 as uuidv4 } from 'uuid';
import { logger } from '../core/logger.js';
import { ToolDefinition } from '../core/types.js';
import { ToolRegistry } from '../tools/registry.js';
import { BaseTool } from '../tools/base.js';
import { ToolContext } from '../core/types.js';

export interface SkillManifest {
  name: string;
  version: string;
  description: string;
  author: string;
  tools: SkillToolDef[];
  triggers: string[];
  dependencies: string[];
  config: Record<string, unknown>;
}

export interface SkillToolDef {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
  riskLevel: 'read_only' | 'write' | 'shell' | 'network';
  command?: string;
  script?: string;
}

class SkillTool extends BaseTool {
  private def: SkillToolDef;
  private workingDir: string;

  constructor(def: SkillToolDef, workingDir: string) {
    super();
    this.def = def;
    this.workingDir = workingDir;
  }

  getDefinition(): ToolDefinition {
    return {
      name: this.def.name,
      description: this.def.description,
      parameters: this.def.parameters,
      riskLevel: this.def.riskLevel,
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    if (this.def.command) {
      const { execFile } = await import('child_process');
      const { promisify } = await import('util');
      const execFileAsync = promisify(execFile);

      let cmd = this.def.command;
      for (const [key, value] of Object.entries(args)) {
        cmd = cmd.replace(`{${key}}`, String(value));
      }

      const parts = cmd.split(/\s+/);
      const bin = parts[0];
      const cmdArgs = parts.slice(1);

      const { stdout, stderr } = await execFileAsync(bin, cmdArgs, {
        cwd: context.workingDir,
        timeout: 120000,
        shell: false,
      });

      return stdout || stderr || 'Command executed successfully';
    }

    if (this.def.script) {
      const scriptPath = path.resolve(this.workingDir, this.def.script);
      const normalizedWorking = path.resolve(this.workingDir);
      if (!scriptPath.startsWith(normalizedWorking)) {
        throw new Error('Script path must be within working directory');
      }
      const scriptContent = await fs.readFile(scriptPath, 'utf-8');

      const vm = await import('vm');
      const sandbox = { args, context, console };
      vm.createContext(sandbox);
      const result = vm.runInContext(scriptContent, sandbox, { timeout: 30000 });
      return String(result);
    }

    return JSON.stringify(args);
  }
}

export class SkillLoader {
  async loadFromToml(filePath: string): Promise<SkillManifest | null> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      return this.parseToml(content);
    } catch (error) {
      logger.error(`Failed to load skill from ${filePath}:`, error);
      return null;
    }
  }

  async loadFromMd(filePath: string): Promise<SkillManifest | null> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      return this.parseMarkdown(content);
    } catch (error) {
      logger.error(`Failed to load skill from ${filePath}:`, error);
      return null;
    }
  }

  private parseToml(content: string): SkillManifest {
    const manifest: SkillManifest = {
      name: '',
      version: '1.0.0',
      description: '',
      author: '',
      tools: [],
      triggers: [],
      dependencies: [],
      config: {},
    };

    let currentSection = '';
    let currentTool: Partial<SkillToolDef> | null = null;

    for (const line of content.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;

      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        currentSection = trimmed.slice(1, -1);
        if (currentSection.startsWith('tool.')) {
          currentTool = { name: currentSection.slice(5) };
        } else {
          currentTool = null;
        }
        continue;
      }

      const eqIdx = trimmed.indexOf('=');
      if (eqIdx < 0) continue;

      const key = trimmed.slice(0, eqIdx).trim();
      let value = trimmed.slice(eqIdx + 1).trim();

      if ((value.startsWith('"') && value.endsWith('"')) ||
          (value.startsWith("'") && value.endsWith("'"))) {
        value = value.slice(1, -1);
      }

      if (currentSection === 'skill') {
        switch (key) {
          case 'name': manifest.name = value; break;
          case 'version': manifest.version = value; break;
          case 'description': manifest.description = value; break;
          case 'author': manifest.author = value; break;
          case 'triggers':
            manifest.triggers = value.split(',').map(s => s.trim());
            break;
          case 'dependencies':
            manifest.dependencies = value.split(',').map(s => s.trim());
            break;
        }
      } else if (currentTool) {
        switch (key) {
          case 'description': currentTool.description = value; break;
          case 'risk_level': currentTool.riskLevel = value as SkillToolDef['riskLevel']; break;
          case 'command': currentTool.command = value; break;
          case 'script': currentTool.script = value; break;
        }
      }
    }

    if (currentTool && currentTool.name) {
      manifest.tools.push(currentTool as SkillToolDef);
    }

    return manifest;
  }

  private parseMarkdown(content: string): SkillManifest {
    const manifest: SkillManifest = {
      name: '',
      version: '1.0.0',
      description: '',
      author: '',
      tools: [],
      triggers: [],
      dependencies: [],
      config: {},
    };

    const lines = content.split('\n');
    let inFrontmatter = false;
    let inCodeBlock = false;
    let currentTool: Partial<SkillToolDef> | null = null;

    for (const line of lines) {
      if (line.trim() === '---') {
        inFrontmatter = !inFrontmatter;
        continue;
      }

      if (inFrontmatter) {
        const colonIdx = line.indexOf(':');
        if (colonIdx > 0) {
          const key = line.slice(0, colonIdx).trim();
          const value = line.slice(colonIdx + 1).trim();
          switch (key) {
            case 'name': manifest.name = value; break;
            case 'version': manifest.version = value; break;
            case 'description': manifest.description = value; break;
            case 'author': manifest.author = value; break;
            case 'triggers':
              manifest.triggers = value.split(',').map(s => s.trim());
              break;
          }
        }
        continue;
      }

      if (line.startsWith('```')) {
        inCodeBlock = !inCodeBlock;
        if (!inCodeBlock && currentTool && currentTool.name) {
          manifest.tools.push(currentTool as SkillToolDef);
          currentTool = null;
        }
        continue;
      }

      if (inCodeBlock && line.startsWith('# ')) {
        currentTool = { name: line.slice(2).trim() };
      }
    }

    return manifest;
  }

  validate(manifest: SkillManifest): boolean {
    if (!manifest.name) {
      logger.warn('Skill manifest missing name');
      return false;
    }
    return true;
  }
}

export class SkillManager {
  private skills: Map<string, SkillManifest> = new Map();
  private toolRegistry: ToolRegistry;
  private loader: SkillLoader;
  private workingDir: string;

  constructor(toolRegistry: ToolRegistry, workingDir: string) {
    this.toolRegistry = toolRegistry;
    this.loader = new SkillLoader();
    this.workingDir = workingDir;
  }

  async load(dir: string): Promise<void> {
    try {
      const entries = await fs.readdir(dir, { withFileTypes: true });

      for (const entry of entries) {
        if (!entry.isDirectory()) continue;

        const skillDir = path.join(dir, entry.name);
        const tomlPath = path.join(skillDir, 'maix-skill.toml');
        const mdPath = path.join(skillDir, 'SKILL.md');

        let manifest: SkillManifest | null = null;

        try {
          await fs.access(tomlPath);
          manifest = await this.loader.loadFromToml(tomlPath);
        } catch {
          try {
            await fs.access(mdPath);
            manifest = await this.loader.loadFromMd(mdPath);
          } catch {
            continue;
          }
        }

        if (manifest && this.loader.validate(manifest)) {
          this.skills.set(manifest.name, manifest);
          this.registerSkillTools(manifest);
          logger.info(`Loaded skill: ${manifest.name} v${manifest.version}`);
        }
      }
    } catch (error) {
      logger.error(`Failed to load skills from ${dir}:`, error);
    }
  }

  private registerSkillTools(manifest: SkillManifest): void {
    for (const toolDef of manifest.tools) {
      const tool = new SkillTool(toolDef, this.workingDir);
      this.toolRegistry.register(tool);
    }
  }

  enable(skillName: string): void {
    const manifest = this.skills.get(skillName);
    if (manifest) {
      this.registerSkillTools(manifest);
      logger.info(`Skill enabled: ${skillName}`);
    }
  }

  disable(skillName: string): void {
    const manifest = this.skills.get(skillName);
    if (manifest) {
      for (const tool of manifest.tools) {
        this.toolRegistry.remove(tool.name);
      }
      logger.info(`Skill disabled: ${skillName}`);
    }
  }

  list(): SkillManifest[] {
    return Array.from(this.skills.values());
  }

  get(name: string): SkillManifest | undefined {
    return this.skills.get(name);
  }

  getTriggers(input: string): SkillManifest[] {
    const lower = input.toLowerCase();
    return Array.from(this.skills.values()).filter(skill =>
      skill.triggers.some(trigger => lower.includes(trigger.toLowerCase()))
    );
  }
}
