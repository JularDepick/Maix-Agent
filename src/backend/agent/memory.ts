import initSqlJs, { Database } from 'sql.js';
import { Message, Session } from '../core/types.js';
import { logger } from '../core/logger.js';
import path from 'path';
import fs from 'fs';
import { Identity } from './identity.js';

export interface MemoryEntry {
  id: string;
  sessionId: string;
  type: 'episodic' | 'semantic';
  content: string;
  importance: number;
  createdAt: number;
  accessedAt: number;
  accessCount: number;
}

export class MemoryStore {
  private db: Database | null = null;
  private dbPath: string;
  private initialized = false;

  constructor(dbPath: string) {
    this.dbPath = dbPath;
  }

  async init(): Promise<void> {
    if (this.initialized) return;

    const dir = path.dirname(this.dbPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const SQL = await initSqlJs();

    if (fs.existsSync(this.dbPath)) {
      const buffer = fs.readFileSync(this.dbPath);
      this.db = new SQL.Database(buffer);
    } else {
      this.db = new SQL.Database();
    }

    this.db.run(`
      CREATE TABLE IF NOT EXISTS memories (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        type TEXT NOT NULL,
        content TEXT NOT NULL,
        importance REAL DEFAULT 0.5,
        created_at INTEGER NOT NULL,
        accessed_at INTEGER NOT NULL,
        access_count INTEGER DEFAULT 0
      )
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        total_tokens INTEGER DEFAULT 0,
        total_cost REAL DEFAULT 0
      )
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        timestamp INTEGER NOT NULL
      )
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS identities (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL,
        system_prompt TEXT NOT NULL,
        traits TEXT DEFAULT '[]',
        capabilities TEXT DEFAULT '[]',
        restrictions TEXT DEFAULT '[]',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      )
    `);

    this.save();
    this.initialized = true;
    logger.info(`Memory store initialized: ${this.dbPath}`);
  }

  private save(): void {
    if (!this.db) return;
    const data = this.db.export();
    fs.writeFileSync(this.dbPath, Buffer.from(data));
  }

  private ensureInitialized(): void {
    if (!this.initialized || !this.db) {
      throw new Error('MemoryStore not initialized. Call init() first.');
    }
  }

  saveSession(session: Session): void {
    this.ensureInitialized();

    this.db!.run(
      `INSERT OR REPLACE INTO sessions (id, name, created_at, updated_at, total_tokens, total_cost)
       VALUES (?, ?, ?, ?, ?, ?)`,
      [session.id, session.name, session.createdAt, session.updatedAt, session.totalTokens, session.totalCost]
    );

    for (const msg of session.messages) {
      this.db!.run(
        `INSERT OR REPLACE INTO messages (id, session_id, role, content, timestamp)
         VALUES (?, ?, ?, ?, ?)`,
        [msg.id, session.id, msg.role, msg.content, msg.timestamp]
      );
    }

    this.save();
    logger.debug(`Saved session: ${session.id}`);
  }

  loadSession(sessionId: string): Session | null {
    this.ensureInitialized();

    const sessionResult = this.db!.exec('SELECT * FROM sessions WHERE id = ?', [sessionId]);
    if (!sessionResult.length || !sessionResult[0].values.length) return null;

    const row = sessionResult[0].values[0];
    const session: Session = {
      id: row[0] as string,
      name: row[1] as string,
      messages: [],
      createdAt: row[2] as number,
      updatedAt: row[3] as number,
      totalTokens: row[4] as number,
      totalCost: row[5] as number,
    };

    const messagesResult = this.db!.exec(
      'SELECT * FROM messages WHERE session_id = ? ORDER BY timestamp',
      [sessionId]
    );

    if (messagesResult.length) {
      session.messages = messagesResult[0].values.map((row) => ({
        id: row[0] as string,
        role: row[2] as 'user' | 'assistant' | 'system' | 'tool',
        content: row[3] as string,
        timestamp: row[4] as number,
      }));
    }

    return session;
  }

  listSessions(): Session[] {
    this.ensureInitialized();

    const result = this.db!.exec('SELECT * FROM sessions ORDER BY updated_at DESC');
    if (!result.length) return [];

    return result[0].values.map((row) => ({
      id: row[0] as string,
      name: row[1] as string,
      messages: [],
      createdAt: row[2] as number,
      updatedAt: row[3] as number,
      totalTokens: row[4] as number,
      totalCost: row[5] as number,
    }));
  }

  deleteSession(sessionId: string): void {
    this.ensureInitialized();

    this.db!.run('DELETE FROM messages WHERE session_id = ?', [sessionId]);
    this.db!.run('DELETE FROM sessions WHERE id = ?', [sessionId]);
    this.save();
    logger.debug(`Deleted session: ${sessionId}`);
  }

  saveMemory(entry: Omit<MemoryEntry, 'id' | 'createdAt' | 'accessedAt' | 'accessCount'>): void {
    this.ensureInitialized();

    const id = `mem_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();

    this.db!.run(
      `INSERT INTO memories (id, session_id, type, content, importance, created_at, accessed_at, access_count)
       VALUES (?, ?, ?, ?, ?, ?, ?, 0)`,
      [id, entry.sessionId, entry.type, entry.content, entry.importance, now, now]
    );

    this.save();
    logger.debug(`Saved memory: ${id}`);
  }

  searchMemories(query: string, limit: number = 10): MemoryEntry[] {
    this.ensureInitialized();

    const result = this.db!.exec(
      `SELECT * FROM memories
       WHERE content LIKE ?
       ORDER BY importance DESC, accessed_at DESC
       LIMIT ?`,
      [`%${query}%`, limit]
    );

    if (!result.length) return [];

    return result[0].values.map((row) => ({
      id: row[0] as string,
      sessionId: row[1] as string,
      type: row[2] as 'episodic' | 'semantic',
      content: row[3] as string,
      importance: row[4] as number,
      createdAt: row[5] as number,
      accessedAt: row[6] as number,
      accessCount: row[7] as number,
    }));
  }

  getContextForSession(sessionId: string, maxTokens: number = 4000): string {
    this.ensureInitialized();

    const result = this.db!.exec(
      `SELECT * FROM memories
       WHERE session_id = ?
       ORDER BY importance DESC, accessed_at DESC
       LIMIT 10`,
      [sessionId]
    );

    if (!result.length) return '';

    let totalTokens = 0;
    const contextParts: string[] = [];

    for (const row of result[0].values) {
      const content = row[3] as string;
      const tokens = Math.ceil(content.length / 4);
      if (totalTokens + tokens > maxTokens) break;

      contextParts.push(content);
      totalTokens += tokens;

      this.db!.run(
        'UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?',
        [Date.now(), row[0]]
      );
    }

    this.save();
    return contextParts.join('\n\n');
  }

  saveIdentity(identity: Identity): void {
    this.ensureInitialized();

    this.db!.run(
      `INSERT OR REPLACE INTO identities (id, name, description, system_prompt, traits, capabilities, restrictions, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        identity.id,
        identity.name,
        identity.description,
        identity.systemPrompt,
        JSON.stringify(identity.traits),
        JSON.stringify(identity.capabilities),
        JSON.stringify(identity.restrictions),
        identity.createdAt,
        identity.updatedAt,
      ]
    );

    this.save();
    logger.debug(`Saved identity: ${identity.id}`);
  }

  searchIdentities(): Identity[] {
    this.ensureInitialized();

    const result = this.db!.exec('SELECT * FROM identities ORDER BY created_at DESC');
    if (!result.length) return [];

    return result[0].values.map((row) => ({
      id: row[0] as string,
      name: row[1] as string,
      description: row[2] as string,
      systemPrompt: row[3] as string,
      traits: JSON.parse(row[4] as string || '[]'),
      capabilities: JSON.parse(row[5] as string || '[]'),
      restrictions: JSON.parse(row[6] as string || '[]'),
      createdAt: row[7] as number,
      updatedAt: row[8] as number,
    }));
  }

  deleteIdentity(id: string): void {
    this.ensureInitialized();

    this.db!.run('DELETE FROM identities WHERE id = ?', [id]);
    this.save();
    logger.debug(`Deleted identity: ${id}`);
  }

  close(): void {
    if (this.db) {
      this.save();
      this.db.close();
      this.db = null;
      this.initialized = false;
    }
  }
}
