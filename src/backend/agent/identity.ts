import { v4 as uuidv4 } from 'uuid';
import { logger } from '../core/logger.js';
import { MemoryStore } from './memory.js';

export interface Identity {
  id: string;
  name: string;
  description: string;
  systemPrompt: string;
  traits: string[];
  capabilities: string[];
  restrictions: string[];
  createdAt: number;
  updatedAt: number;
}

export class IdentityManager {
  private identities: Map<string, Identity> = new Map();
  private memory: MemoryStore;
  private activeIdentityId: string | null = null;

  constructor(memory: MemoryStore) {
    this.memory = memory;
  }

  create(config: Omit<Identity, 'id' | 'createdAt' | 'updatedAt'>): Identity {
    const identity: Identity = {
      ...config,
      id: uuidv4(),
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    this.identities.set(identity.id, identity);
    this.persistIdentity(identity);
    logger.info(`Created identity: ${identity.name} (${identity.id})`);
    return identity;
  }

  update(id: string, updates: Partial<Omit<Identity, 'id' | 'createdAt'>>): Identity | null {
    const identity = this.identities.get(id);
    if (!identity) {
      logger.warn(`Identity not found: ${id}`);
      return null;
    }

    const updated = {
      ...identity,
      ...updates,
      updatedAt: Date.now(),
    };

    if (updates.description || updates.traits || updates.capabilities || updates.restrictions) {
      updated.systemPrompt = this.generateSystemPrompt(updated);
    }

    this.identities.set(id, updated);
    this.persistIdentity(updated);
    logger.info(`Updated identity: ${updated.name} (${id})`);
    return updated;
  }

  delete(id: string): boolean {
    const identity = this.identities.get(id);
    if (!identity) return false;

    this.identities.delete(id);
    if (this.activeIdentityId === id) {
      this.activeIdentityId = null;
    }
    logger.info(`Deleted identity: ${identity.name} (${id})`);
    return true;
  }

  get(id: string): Identity | undefined {
    return this.identities.get(id);
  }

  getByName(name: string): Identity | undefined {
    for (const identity of this.identities.values()) {
      if (identity.name === name) return identity;
    }
    return undefined;
  }

  list(): Identity[] {
    return Array.from(this.identities.values());
  }

  setActive(id: string): boolean {
    const identity = this.identities.get(id);
    if (!identity) return false;
    this.activeIdentityId = id;
    logger.info(`Active identity set to: ${identity.name}`);
    return true;
  }

  getActive(): Identity | undefined {
    if (!this.activeIdentityId) return undefined;
    return this.identities.get(this.activeIdentityId);
  }

  generateSystemPrompt(identity: Identity): string {
    const parts: string[] = [];

    parts.push(`You are ${identity.name}.`);
    parts.push(identity.description);

    if (identity.traits.length > 0) {
      parts.push(`\nPersonality traits: ${identity.traits.join(', ')}.`);
    }

    if (identity.capabilities.length > 0) {
      parts.push(`\nYou are capable of: ${identity.capabilities.join('; ')}.`);
    }

    if (identity.restrictions.length > 0) {
      parts.push(`\nYou must NOT: ${identity.restrictions.join('; ')}.`);
    }

    return parts.join('\n');
  }

  loadFromMemory(): void {
    const result = this.memory.searchIdentities();
    for (const identity of result) {
      this.identities.set(identity.id, identity);
    }
    if (result.length > 0) {
      logger.info(`Loaded ${result.length} identities from database`);
    }
  }

  private persistIdentity(identity: Identity): void {
    this.memory.saveIdentity(identity);
  }
}
