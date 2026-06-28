import { BaseProvider } from './base.js';
import { OpenAIProvider } from './openai.js';
import { AnthropicProvider } from './anthropic.js';
import { ProviderConfig } from '../core/types.js';
import { ConfigError } from '../core/errors.js';
import { logger } from '../core/logger.js';

export class ProviderRegistry {
  private providers: Map<string, BaseProvider> = new Map();
  private defaultProviderName: string;

  constructor(defaultProvider: string) {
    this.defaultProviderName = defaultProvider;
  }

  register(name: string, provider: BaseProvider): void {
    this.providers.set(name, provider);
    logger.info(`Registered provider: ${name} (${provider.getModel()})`);
  }

  get(name?: string): BaseProvider {
    const providerName = name || this.defaultProviderName;
    const provider = this.providers.get(providerName);

    if (!provider) {
      throw new ConfigError(`Provider not found: ${providerName}`);
    }

    return provider;
  }

  getDefault(): BaseProvider {
    return this.get(this.defaultProviderName);
  }

  list(): Array<{ name: string; model: string; type: string }> {
    return Array.from(this.providers.entries()).map(([name, provider]) => ({
      name,
      model: provider.getModel(),
      type: provider.getProviderType(),
    }));
  }

  static fromConfig(config: {
    providers: Record<string, ProviderConfig>;
    defaultProvider: string;
  }): ProviderRegistry {
    const registry = new ProviderRegistry(config.defaultProvider);

    for (const [name, providerConfig] of Object.entries(config.providers)) {
      if (!providerConfig.apiKey) {
        logger.warn(`Skipping provider ${name}: no API key configured`);
        continue;
      }

      let provider: BaseProvider;

      switch (providerConfig.type) {
        case 'openai':
          provider = new OpenAIProvider(providerConfig);
          break;
        case 'anthropic':
          provider = new AnthropicProvider(providerConfig);
          break;
        default:
          logger.warn(`Unknown provider type: ${providerConfig.type}`);
          continue;
      }

      registry.register(name, provider);
    }

    return registry;
  }
}
