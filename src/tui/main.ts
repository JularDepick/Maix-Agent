import { TuiApp } from './app.js';
import { loadConfig, AppConfig } from '../backend/core/config.js';
import { ProviderRegistry } from '../backend/provider/registry.js';
import { MemoryStore } from '../backend/agent/memory.js';
import { ToolRegistry } from '../backend/tools/registry.js';
import { Agent } from '../backend/agent/agent.js';
import { LocalAdapter } from './api/local.js';
import { BackendAPI, AgentEvent, SessionManagerAPI, ProviderAPI, ToolsAPI, ModeManagerAPI, MemoryAPI, Session, AgentMode } from './api/types.js';
import { logger } from '../backend/core/logger.js';
import { execSync } from 'child_process';
import path from 'path';

if (process.platform === 'win32') {
  if (!process.env.TERM) process.env.TERM = 'xterm-256color';
  if (!process.env.COLORTERM) process.env.COLORTERM = 'truecolor';
  try { execSync('cmd /c chcp 65001', { stdio: 'ignore' }); } catch {}

  try {
    const kernel32 = (globalThis as any).Bun?.dlopen ? (globalThis as any).Bun.dlopen('kernel32.dll', {
      GetConsoleMode: { args: ['ptr', 'ptr'], returns: 'bool' },
      SetConsoleMode: { args: ['ptr', 'uint32'], returns: 'bool' },
      GetStdHandle: { args: ['int32'], returns: 'ptr' },
    }) : null;
    if (kernel32) {
      const STD_OUTPUT_HANDLE = -11;
      const ENABLE_VIRTUAL_TERMINAL_PROCESSING = 0x0004;
      const handle = kernel32.GetStdHandle(STD_OUTPUT_HANDLE);
      const mode = new Uint32Array(1);
      const buf = Buffer.from(mode.buffer);
      if (kernel32.GetConsoleMode(handle, buf)) {
        kernel32.SetConsoleMode(handle, mode[0] | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
      }
    }
  } catch {}
}

function createUnconfiguredBackend(): BackendAPI {
  const sessions: Session[] = [];
  let currentSession: Session | null = null;

  const sessionManager: SessionManagerAPI = {
    getCurrentSession: async () => currentSession,
    listSessions: async () => sessions,
    createSession: async (name?: string) => {
      const session: Session = {
        id: `session_${Date.now()}`,
        name: name || `Session ${sessions.length + 1}`,
        messages: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
        totalTokens: 0,
        totalCost: 0,
      };
      sessions.push(session);
      currentSession = session;
      return session;
    },
    clearMessages: async () => {
      if (currentSession) {
        currentSession.messages = [];
        currentSession.updatedAt = Date.now();
      }
    },
  };

  const provider: ProviderAPI = {
    getModel: async () => 'not configured',
  };

  const modeManager: ModeManagerAPI = {
    getCurrentMode: async () => 'agent' as AgentMode,
    getAvailableModes: async () => ['plan', 'agent', 'yolo'] as AgentMode[],
    getModeDescription: (mode: AgentMode) => {
      const descriptions: Record<AgentMode, string> = {
        plan: 'Plan mode - think step by step',
        agent: 'Agent mode - standard tool use',
        yolo: 'YOLO mode - auto-approve all',
      };
      return descriptions[mode];
    },
    switchMode: async () => {},
  };

  const memory: MemoryAPI = {
    saveSession: async () => {},
  };

  return {
    async *run(input: string): AsyncGenerator<AgentEvent> {
      yield { type: 'error', error: 'API Key 未配置。请使用 /config set <KEY> <VALUE> 设置，例如:\n  /config set OPENAI_API_KEY sk-xxx' };
    },
    getSessionManager: () => sessionManager,
    getProvider: () => provider,
    getTools: (): ToolsAPI | null => null,
    getModeManager: () => modeManager,
    getId: () => 'unconfigured',
    getMemory: () => memory,
  };
}

async function createBackend(): Promise<{ backend: BackendAPI; config: AppConfig | null }> {
  let config: AppConfig | null = null;

  try {
    config = loadConfig();
    logger.setLevel(config.logLevel);
  } catch (error) {
    logger.warn(`Config load failed: ${(error as Error).message}. TUI will start without backend.`);
  }

  const memory = new MemoryStore(
    config?.dbPath || path.join(process.cwd(), '.maix-agent', 'data', 'maix.db')
  );
  await memory.init();

  const tools = new ToolRegistry();

  if (!config) {
    return { backend: createUnconfiguredBackend(), config: null };
  }

  const providerRegistry = ProviderRegistry.fromConfig(config);
  let provider;
  try {
    provider = providerRegistry.getDefault();
  } catch {
    logger.warn('No provider available. TUI will start without backend.');
    return { backend: createUnconfiguredBackend(), config };
  }

  const agent = new Agent({
    provider,
    memory,
    tools,
    workingDir: process.cwd(),
  });
  await agent.init();

  return { backend: new LocalAdapter(agent), config };
}

async function main(): Promise<void> {
  const { backend, config } = await createBackend();
  const app = new TuiApp({ backend, config });
  await app.start();
}

main();
