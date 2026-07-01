import { config as dotenvConfig } from 'dotenv';
import { ProviderConfig } from './types.js';
import { ConfigError } from './errors.js';
import { logger } from './logger.js';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';
import os from 'os';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function getExeDir(): string {
  const execDir = path.dirname(process.execPath);
  if (execDir && execDir.length > 1 && execDir !== path.sep && execDir !== '/') {
    return execDir;
  }
  return process.cwd();
}

const EXE_DIR = getExeDir();

const ENV_PREFIX = 'MAIX_AGENT_';

const CONFIG_PRIORITY = [
  { name: 'user', path: path.join(os.homedir(), '.maix-agent', 'config.env') },
  { name: 'project', path: path.join(process.cwd(), '.maix-agent', 'config.env') },
  { name: 'exe', path: path.join(EXE_DIR, '.env') },
  { name: 'cwd', path: path.join(process.cwd(), '.env') },
];

function loadEnvFiles(): void {
  for (const source of CONFIG_PRIORITY) {
    if (fs.existsSync(source.path)) {
      dotenvConfig({ path: source.path, override: true });
      logger.debug(`Loaded config from ${source.name}: ${source.path}`);
    }
  }
}

function getEnv(key: string, defaultValue: string): string;
function getEnv(key: string): string | undefined;
function getEnv(key: string, defaultValue?: string): string | undefined {
  const prefixedKey = `${ENV_PREFIX}${key}`;
  const value = process.env[prefixedKey] || process.env[key];
  return value || defaultValue;
}

export interface AppConfig {
  providers: Record<string, ProviderConfig>;
  defaultProvider: 'openai' | 'anthropic';
  dbPath: string;
  logLevel: 'debug' | 'info' | 'warn' | 'error';
  defaultMode: 'plan' | 'agent' | 'yolo';
  enableModelRouter: boolean;
  enableMCP: boolean;
  mcpServers: Record<string, { command: string; args?: string[] }>;
  skillsDir: string;
  wsPort: number;
}

export function loadConfig(): AppConfig {
  loadEnvFiles();

  const openaiKey = getEnv('OPENAI_API_KEY');
  const anthropicKey = getEnv('ANTHROPIC_API_KEY');

  if (!openaiKey && !anthropicKey) {
    throw new ConfigError('至少需要配置一个 Provider 的 API Key');
  }

  const defaultProvider = (getEnv('DEFAULT_PROVIDER', 'openai')) as 'openai' | 'anthropic';

  if (defaultProvider === 'openai' && !openaiKey) {
    throw new ConfigError('默认 Provider 为 openai，但未配置 OPENAI_API_KEY');
  }

  if (defaultProvider === 'anthropic' && !anthropicKey) {
    throw new ConfigError('默认 Provider 为 anthropic，但未配置 ANTHROPIC_API_KEY');
  }

  const providers: Record<string, ProviderConfig> = {};

  if (openaiKey) {
    providers.openai = {
      type: 'openai',
      apiKey: openaiKey,
      baseUrl: getEnv('OPENAI_BASE_URL', 'https://api.openai.com/v1'),
      model: getEnv('OPENAI_MODEL', 'gpt-4o'),
    };
  }

  if (anthropicKey) {
    providers.anthropic = {
      type: 'anthropic',
      apiKey: anthropicKey,
      model: getEnv('ANTHROPIC_MODEL', 'claude-3-5-sonnet-20241022'),
    };
  }

  const logLevel = getEnv('LOG_LEVEL', 'info');
  if (!['debug', 'info', 'warn', 'error'].includes(logLevel)) {
    throw new ConfigError(`无效的日志级别: ${logLevel}`);
  }

  const defaultMode = (getEnv('DEFAULT_MODE', 'agent')) as 'plan' | 'agent' | 'yolo';
  if (!['plan', 'agent', 'yolo'].includes(defaultMode)) {
    throw new ConfigError(`无效的默认模式: ${defaultMode}`);
  }

  const enableModelRouter = getEnv('ENABLE_MODEL_ROUTER', 'false') === 'true';
  const enableMCP = getEnv('ENABLE_MCP', 'false') === 'true';
  const skillsDir = getEnv('SKILLS_DIR', path.join(EXE_DIR, 'skills'));
  const wsPortRaw = parseInt(getEnv('WS_PORT', '8765'), 10);
  const wsPort = Number.isNaN(wsPortRaw) ? 8765 : wsPortRaw;

  let mcpServers: Record<string, { command: string; args?: string[] }> = {};
  const mcpServersStr = getEnv('MCP_SERVERS');
  if (mcpServersStr) {
    try {
      mcpServers = JSON.parse(mcpServersStr);
    } catch {
      logger.warn('Failed to parse MCP_SERVERS config');
    }
  }

  const dbPath = getEnv('DB_PATH');
  const resolvedDbPath = dbPath
    ? path.resolve(EXE_DIR, dbPath)
    : path.join(EXE_DIR, 'data', 'maix.db');

  return {
    providers,
    defaultProvider,
    dbPath: resolvedDbPath,
    logLevel: logLevel as 'debug' | 'info' | 'warn' | 'error',
    defaultMode,
    enableModelRouter,
    enableMCP,
    mcpServers,
    skillsDir,
    wsPort,
  };
}