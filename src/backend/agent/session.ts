import { v4 as uuidv4 } from 'uuid';
import { Session, Message, Role } from '../core/types.js';
import { MemoryStore } from './memory.js';
import { logger } from '../core/logger.js';

export class SessionManager {
  private sessions: Map<string, Session> = new Map();
  private currentSessionId: string | null = null;

  loadSessions(memory: MemoryStore): void {
    const savedSessions = memory.listSessions();
    for (const session of savedSessions) {
      const fullSession = memory.loadSession(session.id);
      if (fullSession) {
        this.sessions.set(fullSession.id, fullSession);
      }
    }
    if (savedSessions.length > 0) {
      this.currentSessionId = savedSessions[0].id;
      logger.info(`Loaded ${savedSessions.length} sessions from database`);
    }
  }

  setCurrentSession(id: string): Session | undefined {
    const session = this.sessions.get(id);
    if (session) {
      this.currentSessionId = id;
      logger.info(`Set current session: ${id}`);
    }
    return session;
  }

  createSession(name?: string): Session {
    const id = uuidv4();
    const session: Session = {
      id,
      name: name || `Session ${this.sessions.size + 1}`,
      messages: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      totalTokens: 0,
      totalCost: 0,
    };

    this.sessions.set(id, session);
    this.currentSessionId = id;

    logger.info(`Created session: ${id}`);
    return session;
  }

  getSession(id: string): Session | undefined {
    return this.sessions.get(id);
  }

  getCurrentSession(): Session | undefined {
    if (!this.currentSessionId) return undefined;
    return this.sessions.get(this.currentSessionId);
  }

  switchSession(id: string): Session | undefined {
    const session = this.sessions.get(id);
    if (session) {
      this.currentSessionId = id;
      logger.info(`Switched to session: ${id}`);
    }
    return session;
  }

  deleteSession(id: string): boolean {
    const deleted = this.sessions.delete(id);
    if (deleted && this.currentSessionId === id) {
      this.currentSessionId = this.sessions.keys().next().value || null;
    }
    return deleted;
  }

  listSessions(): Session[] {
    return Array.from(this.sessions.values()).sort((a, b) => b.updatedAt - a.updatedAt);
  }

  addMessage(role: Role, content: string, message?: Partial<Message>): Message {
    const session = this.getCurrentSession();
    if (!session) {
      throw new Error('No active session');
    }

    const msg: Message = {
      id: uuidv4(),
      role,
      content,
      timestamp: Date.now(),
      ...message,
    };

    session.messages.push(msg);
    session.updatedAt = Date.now();

    if (role === 'user' && session.messages.length === 1) {
      session.name = content.slice(0, 50) + (content.length > 50 ? '...' : '');
    }

    return msg;
  }

  updateLastMessage(updates: Partial<Message>): void {
    const session = this.getCurrentSession();
    if (!session || session.messages.length === 0) return;

    const lastMessage = session.messages[session.messages.length - 1];
    Object.assign(lastMessage, updates);
    session.updatedAt = Date.now();
  }

  getMessages(limit?: number): Message[] {
    const session = this.getCurrentSession();
    if (!session) return [];

    if (limit) {
      return session.messages.slice(-limit);
    }
    return session.messages;
  }

  clearMessages(): void {
    const session = this.getCurrentSession();
    if (!session) return;

    session.messages = [];
    session.updatedAt = Date.now();
    logger.info(`Cleared messages for session: ${session.id}`);
  }

  updateTokenUsage(promptTokens: number, completionTokens: number, cost: number): void {
    const session = this.getCurrentSession();
    if (!session) return;

    session.totalTokens += promptTokens + completionTokens;
    session.totalCost += cost;
  }
}
