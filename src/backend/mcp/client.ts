import { spawn, ChildProcess } from 'child_process';
import { MCPRequest, MCPResponse, MCPNotification, MCPTransport, MCPTool, MCPResource, MCPPrompt } from './types.js';
import { logger } from '../core/logger.js';
import { ToolDefinition } from '../core/types.js';
import { BaseTool } from '../tools/base.js';
import { ToolContext } from '../core/types.js';
import { ToolError } from '../core/errors.js';

export class StdioTransport extends MCPTransport {
  private process: ChildProcess | null = null;
  private requestId = 0;
  private pendingRequests: Map<number | string, {
    resolve: (response: MCPResponse) => void;
    reject: (error: Error) => void;
  }> = new Map();
  private notificationHandler: ((notification: MCPNotification) => void) | null = null;
  private buffer = '';

  constructor(
    private command: string,
    private args: string[] = []
  ) {
    super();
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.process = spawn(this.command, this.args, {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      this.process.on('error', (error) => {
        logger.error('MCP process error:', error);
        reject(error);
      });

      this.process.on('close', (code) => {
        logger.info(`MCP process closed with code ${code}`);
        this.rejectAllPending('Process closed');
      });

      this.process.stdout?.on('data', (data: Buffer) => {
        this.buffer += data.toString();
        this.processMessages();
      });

      this.process.stderr?.on('data', (data: Buffer) => {
        logger.debug(`MCP stderr: ${data.toString()}`);
      });

      this.process.on('spawn', () => {
        setTimeout(() => resolve(), 500);
      });
    });
  }

  async disconnect(): Promise<void> {
    if (this.process) {
      this.process.kill();
      this.process = null;
    }
    this.rejectAllPending('Disconnected');
  }

  async send(request: MCPRequest): Promise<MCPResponse> {
    return new Promise((resolve, reject) => {
      if (!this.process || !this.process.stdin) {
        reject(new Error('Not connected'));
        return;
      }

      const id = request.id;
      const timer = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`MCP request timeout: ${request.method}`));
      }, 30000);

      this.pendingRequests.set(id, {
        resolve: (response) => {
          clearTimeout(timer);
          resolve(response);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });

      const message = JSON.stringify(request) + '\n';
      this.process.stdin.write(message);
    });
  }

  onNotification(handler: (notification: MCPNotification) => void): void {
    this.notificationHandler = handler;
  }

  private processMessages(): void {
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const message = JSON.parse(line);

        if ('id' in message) {
          const pending = this.pendingRequests.get(message.id);
          if (pending) {
            this.pendingRequests.delete(message.id);
            pending.resolve(message as MCPResponse);
          }
        } else if ('method' in message) {
          this.notificationHandler?.(message as MCPNotification);
        }
      } catch (error) {
        logger.error('Failed to parse MCP message:', error);
      }
    }
  }

  private rejectAllPending(reason: string): void {
    for (const [id, pending] of this.pendingRequests) {
      pending.reject(new Error(reason));
    }
    this.pendingRequests.clear();
  }
}

export class MCPClient {
  private transport: MCPTransport;
  private requestId = 0;
  private connected = false;
  private serverCapabilities: Record<string, unknown> = {};

  constructor(transport: MCPTransport) {
    this.transport = transport;
  }

  async connect(): Promise<void> {
    await this.transport.connect();
    this.connected = true;

    const response = await this.sendRequest('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {
        tools: {},
        resources: {},
        prompts: {},
      },
      clientInfo: {
        name: 'maix-agent',
        version: '1.0.1',
      },
    });

    this.serverCapabilities = (response.result as Record<string, unknown>)?.capabilities as Record<string, unknown> || {};
    logger.info('MCP client connected');
  }

  async disconnect(): Promise<void> {
    await this.transport.disconnect();
    this.connected = false;
  }

  async listTools(): Promise<MCPTool[]> {
    const response = await this.sendRequest('tools/list', {});
    return (response.result as { tools: MCPTool[] })?.tools || [];
  }

  async callTool(name: string, args: unknown): Promise<unknown> {
    const response = await this.sendRequest('tools/call', { name, arguments: args });
    return response.result;
  }

  async listResources(): Promise<MCPResource[]> {
    const response = await this.sendRequest('resources/list', {});
    return (response.result as { resources: MCPResource[] })?.resources || [];
  }

  async readResource(uri: string): Promise<unknown> {
    const response = await this.sendRequest('resources/read', { uri });
    return response.result;
  }

  async listPrompts(): Promise<MCPPrompt[]> {
    const response = await this.sendRequest('prompts/list', {});
    return (response.result as { prompts: MCPPrompt[] })?.prompts || [];
  }

  async getPrompt(name: string, args?: Record<string, string>): Promise<unknown> {
    const response = await this.sendRequest('prompts/get', { name, arguments: args });
    return response.result;
  }

  isConnected(): boolean {
    return this.connected;
  }

  getServerCapabilities(): Record<string, unknown> {
    return this.serverCapabilities;
  }

  private async sendRequest(method: string, params: unknown): Promise<MCPResponse> {
    const request: MCPRequest = {
      jsonrpc: '2.0',
      id: ++this.requestId,
      method,
      params,
    };

    return this.transport.send(request);
  }
}

export class MCPToolAdapter extends BaseTool {
  private client: MCPClient;
  private toolName: string;
  private toolDescription: string;
  private toolSchema: Record<string, unknown>;

  constructor(client: MCPClient, toolName: string, description: string, schema: Record<string, unknown>) {
    super();
    this.client = client;
    this.toolName = toolName;
    this.toolDescription = description;
    this.toolSchema = schema;
  }

  getDefinition(): ToolDefinition {
    return {
      name: `mcp_${this.toolName}`,
      description: this.toolDescription,
      parameters: this.toolSchema,
      riskLevel: 'network',
    };
  }

  async execute(args: Record<string, unknown>, context: ToolContext): Promise<string> {
    try {
      const result = await this.client.callTool(this.toolName, args);
      return typeof result === 'string' ? result : JSON.stringify(result, null, 2);
    } catch (error) {
      throw new ToolError(`MCP tool ${this.toolName} failed: ${(error as Error).message}`, error as Error);
    }
  }
}
