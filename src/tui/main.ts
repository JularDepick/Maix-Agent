import { TuiApp } from './app.js';
import { loadConfig, AppConfig } from '../backend/core/config.js';
import { ProviderRegistry } from '../backend/provider/registry.js';
import { MemoryStore } from '../backend/agent/memory.js';
import { ToolRegistry } from '../backend/tools/registry.js';
import { Agent } from '../backend/agent/agent.js';
import { LocalAdapter } from './api/local.js';
import { BackendAPI, AgentEvent } from './api/types.js';
import { logger } from '../backend/core/logger.js';

function createStubBackend(): BackendAPI {
  return {
    async *run(): AsyncGenerator<AgentEvent> {
      yield { type: 'error', error: 'Backend not configured. Use /config to set up API Key.' };
    },
    getSessionManager() {
      return {
        getCurrentSession: async () => null,
        listSessions: async () => [],
        createSession: async () => ({ id: '', name: '', messages: [], createdAt: 0, updatedAt: 0, totalTokens: 0, totalCost: 0 }),
        clearMessages: async () => {},
      };
    },
    getProvider() {
      return { getModel: async () => 'not configured' };
    },
    getTools() {
      return null;
    },
    getModeManager() {
      return {
        getCurrentMode: async () => 'agent' as const,
        getAvailableModes: async () => ['plan', 'agent', 'yolo'] as const,
        getModeDescription: () => '',
        switchMode: async () => {},
      };
    },
    getId() {
      return 'stub';
    },
    getMemory() {
      return { saveSession: async () => {} };
    },
  };
}

async function createBackend(): Promise<{ backend: BackendAPI; config: AppConfig | null }> {
  try {
    const config = loadConfig();
    logger.setLevel(config.logLevel);

    const providerRegistry = ProviderRegistry.fromConfig(config);
    const provider = providerRegistry.getDefault();

    const memory = new MemoryStore(config.dbPath);
    await memory.init();

    const tools = new ToolRegistry();

    const agent = new Agent({
      provider,
      memory,
      tools,
      workingDir: process.cwd(),
    });
    await agent.init();

    return { backend: new LocalAdapter(agent), config };
  } catch (error) {
    logger.warn(`Backend init failed: ${(error as Error).message}. Running in stub mode.`);
    return { backend: createStubBackend(), config: null };
  }
}

async function main(): Promise<void> {
  const { backend, config } = await createBackend();
  const app = new TuiApp({ backend, config });
  await app.start();
}

main();
