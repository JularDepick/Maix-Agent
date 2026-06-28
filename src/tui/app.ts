import { ThemeManager } from './themes/index.js';
import { StatusPanel } from './panels/status.js';
import { BackendAPI, Session, AgentEvent, AgentMode, ToolCall } from './api/types.js';
import { AppConfig } from '../backend/core/config.js';
import { term } from './terminal.js';
import fs from 'fs';
import path from 'path';

export interface TuiConfig {
  backend: BackendAPI;
  config: AppConfig | null;
}

export class TuiApp {
  private backend: BackendAPI;
  private appConfig: AppConfig | null;
  private theme: ThemeManager;
  private statusPanel: StatusPanel;
  private inputBuffer: string = '';
  private cursorPos: number = 0;
  private isProcessing: boolean = false;
  private messageHistory: string[] = [];
  private historyIndex: number = -1;
  private showReasoning: boolean = true;
  private foldedMessages: Map<string, boolean> = new Map();
  private autoSaveTimer: ReturnType<typeof setInterval> | null = null;
  private startTime: number = 0;
  private tokenCount: number = 0;

  constructor(config: TuiConfig) {
    this.backend = config.backend;
    this.appConfig = config.config;
    this.theme = new ThemeManager();
    this.statusPanel = new StatusPanel(this.theme);
  }

  async start(): Promise<void> {
    const cols = Math.max(process.stdout.columns || 80, 1);
    const rows = Math.max(process.stdout.rows || 24, 1);
    if (!term.width || term.width <= 0) term.width = cols;
    if (!term.height || term.height <= 0) term.height = rows;

    term.clear();
    term.grabInput(true);
    term.hideCursor(false);

    this.renderHeader();
    this.renderStatusBar();
    this.renderChatArea();
    this.renderInput();

    this.autoSaveTimer = setInterval(() => this.autoSave(), 30000);

    term.on('key', (name: string) => this.handleKey(name));

    await this.waitForExit();
  }

  private async autoSave(): Promise<void> {
    const sessions = this.backend.getSessionManager();
    const session = await sessions.getCurrentSession();
    if (session && session.messages.length > 0) {
      this.backend.getMemory().saveSession(session);
    }
  }

  private renderHeader(): void {
    const theme = this.theme.getTheme();
    term.moveTo(1, 1);
    term.bgColorHex(theme.bg);
    term.colorHex(theme.accent);
    term.bold(' Maix-Agent TUI ');
    term.colorHex(theme.dim);
    term(' v1.0.1');
    term.eraseLine();
  }

  private async renderStatusBar(): Promise<void> {
    const theme = this.theme.getTheme();
    const sessions = this.backend.getSessionManager();
    const provider = this.backend.getProvider();
    const session = await sessions.getCurrentSession();

    term.moveTo(1, 2);
    term.bgColorHex(theme.border);
    term.colorHex(theme.fg);
    term(` Provider: ${await provider.getModel()}`);
    term.colorHex(theme.dim);
    term(` | Session: ${session?.name || 'None'}`);
    term(` | Messages: ${session?.messages.length || 0}`);

    if (this.tokenCount > 0 && this.startTime > 0) {
      const elapsed = (Date.now() - this.startTime) / 1000;
      const tps = this.tokenCount / elapsed;
      term(` | ${tps.toFixed(1)} tok/s`);
    }

    if (session?.totalCost) {
      term(` | $${session.totalCost.toFixed(4)}`);
    }

    term.eraseLine();
  }

  private async renderChatArea(): Promise<void> {
    const theme = this.theme.getTheme();
    const sessions = this.backend.getSessionManager();
    const session = await sessions.getCurrentSession();
    if (!session) return;

    const startRow = 3;
    const endRow = (Number(term.height) || 24) - 3;

    for (let i = startRow; i <= endRow; i++) {
      term.moveTo(1, i);
      term.eraseLine();
    }

    let currentRow = startRow;
    const messages = session.messages;

    for (const msg of messages) {
      if (currentRow > endRow) break;

      term.moveTo(1, currentRow);

      if (msg.role === 'user') {
        term.colorHex(theme.userMsg);
        term.bold('You: ');
      } else if (msg.role === 'assistant') {
        term.colorHex(theme.assistantMsg);
        term.bold('Maix: ');
      } else if (msg.role === 'system') {
        term.colorHex(theme.systemMsg);
        term.dim('[System] ');
      } else if (msg.role === 'tool') {
        term.colorHex(theme.dim);
        term.dim('[Tool] ');
      }

      const isFolded = this.foldedMessages.get(msg.id) !== false;
      const lines = this.wrapText(msg.content, (Number(term.width) || 80) - 8);

      if (lines.length > 50 && isFolded) {
        for (let i = 0; i < 5; i++) {
          if (currentRow > endRow) break;
          term(lines[i]);
          term.eraseLine();
          currentRow++;
          if (currentRow <= endRow) term.moveTo(1, currentRow);
        }
        term.colorHex(theme.dim);
        term(`  [... ${lines.length - 5} more lines, Enter to expand]`);
        term.eraseLine();
        currentRow++;
      } else {
        for (const line of lines) {
          if (currentRow > endRow) break;
          term(line);
          term.eraseLine();
          currentRow++;
          if (currentRow <= endRow) term.moveTo(1, currentRow);
        }
      }
      currentRow++;
    }
  }

  private renderInput(): void {
    const theme = this.theme.getTheme();
    const inputRow = Math.max((Number(term.height) || 24) - 1, 1);
    const width = Math.max(Number(term.width) || 80, 1);

    term.moveTo(1, inputRow - 1);
    term.bgColorHex(theme.border);
    try {
      term('─'.repeat(width));
    } catch {
      term('─'.repeat(80));
    }
    term.eraseLine();

    term.moveTo(1, inputRow);
    term.bgColorHex(theme.bg);
    term.colorHex(theme.accent);
    term.bold('> ');
    term.colorHex(theme.fg);
    term(this.inputBuffer);
    term.eraseLine();
  }

  private async handleKey(name: string): Promise<void> {
    if (this.isProcessing && name !== 'CTRL_C') return;

    switch (name) {
      case 'ENTER':
        await this.handleSubmit();
        break;
      case 'BACKSPACE':
        this.handleBackspace();
        break;
      case 'DELETE':
        this.handleDelete();
        break;
      case 'LEFT':
        this.handleLeft();
        break;
      case 'RIGHT':
        this.handleRight();
        break;
      case 'UP':
        this.handleUp();
        break;
      case 'DOWN':
        this.handleDown();
        break;
      case 'HOME':
        this.cursorPos = 0;
        this.renderInput();
        break;
      case 'END':
        this.cursorPos = this.inputBuffer.length;
        this.renderInput();
        break;
      case 'CTRL_C':
        this.cleanup();
        process.exit(0);
      case 'CTRL_L':
        term.clear();
        this.renderHeader();
        this.renderStatusBar();
        this.renderChatArea();
        this.renderInput();
        break;
      case 'CTRL_U':
        this.inputBuffer = '';
        this.cursorPos = 0;
        this.renderInput();
        break;
      case 'CTRL_R':
        this.showReasoning = !this.showReasoning;
        this.renderChatArea();
        break;
      case 'CTRL_T':
        this.statusPanel.toggle();
        if (!this.statusPanel.isVisible()) {
          this.renderChatArea();
          this.renderInput();
        }
        break;
      case 'CTRL_M':
        this.cycleMode();
        break;
      default:
        if (name.length === 1) {
          this.handleChar(name);
        }
        break;
    }
  }

  private handleChar(char: string): void {
    this.inputBuffer =
      this.inputBuffer.slice(0, this.cursorPos) +
      char +
      this.inputBuffer.slice(this.cursorPos);
    this.cursorPos++;
    this.renderInput();
  }

  private handleBackspace(): void {
    if (this.cursorPos > 0) {
      this.inputBuffer =
        this.inputBuffer.slice(0, this.cursorPos - 1) +
        this.inputBuffer.slice(this.cursorPos);
      this.cursorPos--;
      this.renderInput();
    }
  }

  private handleDelete(): void {
    if (this.cursorPos < this.inputBuffer.length) {
      this.inputBuffer =
        this.inputBuffer.slice(0, this.cursorPos) +
        this.inputBuffer.slice(this.cursorPos + 1);
      this.renderInput();
    }
  }

  private handleLeft(): void {
    if (this.cursorPos > 0) {
      this.cursorPos--;
      this.renderInput();
    }
  }

  private handleRight(): void {
    if (this.cursorPos < this.inputBuffer.length) {
      this.cursorPos++;
      this.renderInput();
    }
  }

  private handleUp(): void {
    if (this.messageHistory.length > 0) {
      if (this.historyIndex < this.messageHistory.length - 1) {
        this.historyIndex++;
        this.inputBuffer = this.messageHistory[this.messageHistory.length - 1 - this.historyIndex];
        this.cursorPos = this.inputBuffer.length;
        this.renderInput();
      }
    }
  }

  private handleDown(): void {
    if (this.historyIndex > 0) {
      this.historyIndex--;
      this.inputBuffer = this.messageHistory[this.messageHistory.length - 1 - this.historyIndex];
      this.cursorPos = this.inputBuffer.length;
    } else {
      this.historyIndex = -1;
      this.inputBuffer = '';
      this.cursorPos = 0;
    }
    this.renderInput();
  }

  private async handleSubmit(): Promise<void> {
    const input = this.inputBuffer.trim();
    if (!input) return;

    this.messageHistory.push(input);
    this.historyIndex = -1;
    this.inputBuffer = '';
    this.cursorPos = 0;

    if (input.startsWith('/')) {
      await this.handleCommand(input);
      return;
    }

    this.isProcessing = true;
    this.startTime = Date.now();
    this.tokenCount = 0;
    await this.renderStatusBar();
    this.renderInput();

    const theme = this.theme.getTheme();

    await this.renderChatArea();
    let responseRow = await this.getChatLines() + 3;
    term.moveTo(1, responseRow);
    term.colorHex(theme.assistantMsg);
    term.bold('Maix: ');

    try {
      for await (const event of this.backend.run(input)) {
        if (event.type === 'text' && event.content) {
          term(event.content);
          this.tokenCount++;
        } else if (event.type === 'reasoning' && event.content && this.showReasoning) {
          term.colorHex(theme.dim);
          term(`\n[Thinking] ${event.content}`);
          term.colorHex(theme.assistantMsg);
        } else if (event.type === 'tool_approval' && event.toolCall) {
          const approved = await this.requestToolApproval(event.toolCall);
          // Note: For remote backends, tool approval is handled differently
        } else if (event.type === 'tool_call' && event.toolCall) {
          term.colorHex(theme.dim);
          term(`\n[Calling: ${event.toolCall.name}]`);
          term.colorHex(theme.assistantMsg);
        } else if (event.type === 'tool_result' && event.toolResult) {
          const duration = event.toolResult.duration ? ` (${event.toolResult.duration}ms)` : '';
          if (event.toolResult.error) {
            term.colorHex(theme.error);
            term(`\n[Error: ${event.toolResult.error}${duration}]`);
          } else {
            term.colorHex(theme.success);
            term(`\n[Done${duration}]`);
          }
          term.colorHex(theme.assistantMsg);
        } else if (event.type === 'error') {
          term.colorHex(theme.error);
          term(`\nError: ${event.error}`);
        }
      }

      term('\n');
    } catch (error) {
      term.colorHex(theme.error);
      term(`\nError: ${(error as Error).message}`);
    }

    this.isProcessing = false;
    await this.renderStatusBar();
    this.renderInput();
  }

  private async requestToolApproval(toolCall: ToolCall): Promise<boolean> {
    const theme = this.theme.getTheme();

    term.colorHex(theme.warn);
    term(`\n[Tool: ${toolCall.name}]`);
    term.colorHex(theme.dim);
    term(` ${JSON.stringify(toolCall.arguments).slice(0, 100)}`);
    term.colorHex(theme.warn);
    term('\nApprove? [Y]es / [N]o / [A]ll: ');

    return new Promise((resolve) => {
      term.inputField({}, (error: Error | null, input: string | undefined) => {
        const choice = (input || '').toUpperCase().trim();
        term('\n');

        if (choice === 'A') {
          this.backend.getTools()?.autoApproveTool(toolCall.name);
          resolve(true);
        } else {
          resolve(choice === 'Y');
        }
      });
    });
  }

  private async handleCommand(command: string): Promise<void> {
    const theme = this.theme.getTheme();
    const cmd = command.slice(1).split(' ')[0];
    const args = command.slice(1).split(' ').slice(1);

    switch (cmd) {
      case 'help':
        this.showHelp();
        break;
      case 'clear':
        await this.backend.getSessionManager().clearMessages();
        term.clear();
        this.renderHeader();
        await this.renderStatusBar();
        this.renderInput();
        break;
      case 'sessions':
        await this.showSessions();
        break;
      case 'new':
        await this.backend.getSessionManager().createSession(args.join(' ') || undefined);
        term.clear();
        this.renderHeader();
        await this.renderStatusBar();
        this.renderInput();
        break;
      case 'model':
        term.colorHex(theme.accent);
        term(`Current model: ${await this.backend.getProvider().getModel()}\n`);
        break;
      case 'cost':
        await this.showCost();
        break;
      case 'theme':
        if (args[0]) {
          this.theme.setTheme(args[0]);
          term.clear();
          this.renderHeader();
          await this.renderStatusBar();
          await this.renderChatArea();
          this.renderInput();
        } else {
          term.colorHex(theme.accent);
          term(`Available themes: ${this.theme.listThemes().join(', ')}\n`);
        }
        break;
      case 'reasoning':
        this.showReasoning = !this.showReasoning;
        term.colorHex(theme.accent);
        term(`Reasoning display: ${this.showReasoning ? 'ON' : 'OFF'}\n`);
        break;
      case 'mode':
        if (args[0]) {
          const mode = args[0] as AgentMode;
          if (['plan', 'agent', 'yolo'].includes(mode)) {
            await this.backend.getModeManager().switchMode(mode);
            term.colorHex(theme.accent);
            term(`Mode switched to: ${mode.toUpperCase()}\n`);
          } else {
            term.colorHex(theme.error);
            term('Invalid mode. Available: plan, agent, yolo\n');
          }
        } else {
          term.colorHex(theme.accent);
          term.bold('Available modes:\n');
          const modeManager = this.backend.getModeManager();
          for (const m of await modeManager.getAvailableModes()) {
            const currentMode = await modeManager.getCurrentMode();
            const isCurrent = m === currentMode;
            term.colorHex(isCurrent ? theme.success : theme.fg);
            term(`  ${isCurrent ? '>' : ' '} ${m.toUpperCase()} - ${modeManager.getModeDescription(m)}\n`);
          }
        }
        break;
      case 'status':
        this.statusPanel.toggle();
        if (this.statusPanel.isVisible()) {
          await this.renderChatArea();
          this.renderInput();
        }
        break;
      case 'config':
        await this.handleConfig(args);
        break;
      case 'exit':
      case 'quit':
        this.cleanup();
        process.exit(0);
      default:
        term.colorHex(theme.error);
        term(`Unknown command: ${cmd}\n`);
        term.colorHex(theme.dim);
        term('Type /help for available commands\n');
    }
  }

  private async handleConfig(args: string[]): Promise<void> {
    const theme = this.theme.getTheme();
    const subcmd = args[0] || 'status';

    switch (subcmd) {
      case 'status':
        if (this.appConfig) {
          term.colorHex(theme.success);
          term('Backend configured\n');
          term.colorHex(theme.fg);
          term(`  Provider: ${this.appConfig.defaultProvider}\n`);
          term(`  Model: ${await this.backend.getProvider().getModel()}\n`);
        } else {
          term.colorHex(theme.warn);
          term('Backend not configured\n');
          term.colorHex(theme.dim);
          term('  Use /config set <key> <value> to configure\n');
        }
        break;
      case 'set':
        if (args.length < 3) {
          term.colorHex(theme.error);
          term('Usage: /config set <key> <value>\n');
          term.colorHex(theme.dim);
          term('Keys: OPENAI_API_KEY, ANTHROPIC_API_KEY, DEFAULT_PROVIDER\n');
          return;
        }
        const key = args[1];
        const value = args.slice(2).join(' ');
        this.setEnvValue(key, value);
        term.colorHex(theme.success);
        term(`Set ${key} = ${key.includes('KEY') ? '***' : value}\n`);
        term.colorHex(theme.dim);
        term('Restart to apply changes\n');
        break;
      case 'show':
        this.showEnvFile();
        break;
      default:
        term.colorHex(theme.error);
        term('Usage: /config [status|set|show]\n');
    }
  }

  private setEnvValue(key: string, value: string): void {
    const envPath = path.join(process.cwd(), '.env');
    let content = '';
    if (fs.existsSync(envPath)) {
      content = fs.readFileSync(envPath, 'utf-8');
    }

    const prefix = key.startsWith('MAIX_AGENT_') ? '' : 'MAIX_AGENT_';
    const fullKey = `${prefix}${key}`;

    const regex = new RegExp(`^${fullKey}=.*$`, 'm');
    if (regex.test(content)) {
      content = content.replace(regex, `${fullKey}=${value}`);
    } else {
      content += `\n${fullKey}=${value}`;
    }

    fs.writeFileSync(envPath, content.trim() + '\n');
  }

  private showEnvFile(): void {
    const theme = this.theme.getTheme();
    const envPath = path.join(process.cwd(), '.env');
    if (!fs.existsSync(envPath)) {
      term.colorHex(theme.warn);
      term('.env file not found\n');
      return;
    }
    const content = fs.readFileSync(envPath, 'utf-8');
    term.colorHex(theme.fg);
    for (const line of content.split('\n')) {
      if (line.startsWith('#') || !line.trim()) {
        term.colorHex(theme.dim);
      } else if (line.includes('KEY')) {
        const parts = line.split('=');
        term(`${parts[0]}=***\n`);
        term.colorHex(theme.fg);
      } else {
        term.colorHex(theme.fg);
        term(`${line}\n`);
      }
    }
  }

  private showHelp(): void {
    const theme = this.theme.getTheme();
    term.colorHex(theme.accent);
    term.bold('Available commands:\n');
    term.colorHex(theme.fg);
    term('  /help           - Show this help\n');
    term('  /clear          - Clear current session\n');
    term('  /sessions       - List all sessions\n');
    term('  /new [name]     - Create new session\n');
    term('  /model          - Show current model\n');
    term('  /cost           - Show token usage and cost\n');
    term('  /theme [name]   - Set or list themes\n');
    term('  /reasoning      - Toggle reasoning display\n');
    term('  /mode [name]    - Switch mode (plan/agent/yolo)\n');
    term('  /status         - Toggle status panel\n');
    term('  /config [cmd]   - Configure settings (status/set/show)\n');
    term('  /exit           - Exit application\n');
    term('\n');
    term.colorHex(theme.dim);
    term('Keyboard shortcuts:\n');
    term('  Ctrl+C          - Exit\n');
    term('  Ctrl+L          - Clear screen\n');
    term('  Ctrl+U          - Clear input\n');
    term('  Ctrl+R          - Toggle reasoning\n');
    term('  Ctrl+T          - Toggle status panel\n');
    term('  Ctrl+M          - Cycle mode\n');
    term('  Up/Down         - Navigate history\n');
    term('\n');
  }

  private async showSessions(): Promise<void> {
    const theme = this.theme.getTheme();
    const sessions = this.backend.getSessionManager();
    const sessionList = await sessions.listSessions();
    const currentSession = await sessions.getCurrentSession();

    term.colorHex(theme.accent);
    term.bold('Sessions:\n');

    for (const session of sessionList) {
      const isCurrent = session.id === currentSession?.id;
      term.colorHex(isCurrent ? theme.success : theme.fg);
      term(`  ${isCurrent ? '>' : ' '} ${session.id.slice(0, 8)}... - ${session.name}`);
      term.colorHex(theme.dim);
      term(` (${session.messages.length} messages)\n`);
    }
    term('\n');
  }

  private async showCost(): Promise<void> {
    const theme = this.theme.getTheme();
    const sessions = this.backend.getSessionManager();
    const session = await sessions.getCurrentSession();

    if (!session) {
      term.colorHex(theme.error);
      term('No active session\n');
      return;
    }

    term.colorHex(theme.accent);
    term.bold('Session Statistics:\n');
    term.colorHex(theme.fg);
    term(`  Messages: ${session.messages.length}\n`);
    term(`  Total Tokens: ${session.totalTokens}\n`);
    term(`  Total Cost: $${session.totalCost.toFixed(4)}\n`);

    if (this.tokenCount > 0 && this.startTime > 0) {
      const elapsed = (Date.now() - this.startTime) / 1000;
      term(`  Speed: ${(this.tokenCount / elapsed).toFixed(1)} tokens/sec\n`);
    }
    term('\n');
  }

  private async getChatLines(): Promise<number> {
    const sessions = this.backend.getSessionManager();
    const session = await sessions.getCurrentSession();
    if (!session) return 0;

    let lines = 0;
    for (const msg of session.messages) {
      lines += Math.ceil(msg.content.length / ((Number(term.width) || 80) - 8)) + 2;
    }
    return lines;
  }

  private wrapText(text: string, maxWidth: number): string[] {
    const lines: string[] = [];
    const words = text.split(' ');
    let currentLine = '';

    for (const word of words) {
      if (currentLine.length + word.length + 1 > maxWidth) {
        lines.push(currentLine);
        currentLine = word;
      } else {
        currentLine += (currentLine ? ' ' : '') + word;
      }
    }

    if (currentLine) {
      lines.push(currentLine);
    }

    return lines;
  }

  private async cycleMode(): Promise<void> {
    const modeManager = this.backend.getModeManager();
    const modes = await modeManager.getAvailableModes();
    const currentMode = await modeManager.getCurrentMode();
    const currentIdx = modes.indexOf(currentMode);
    const nextIdx = (currentIdx + 1) % modes.length;
    const nextMode = modes[nextIdx];

    await modeManager.switchMode(nextMode);

    const theme = this.theme.getTheme();
    term.colorHex(theme.accent);
    term(`\nMode: ${nextMode.toUpperCase()}\n`);
    await this.renderStatusBar();
  }

  private cleanup(): void {
    if (this.autoSaveTimer) {
      clearInterval(this.autoSaveTimer);
    }
    this.autoSave();
    term.grabInput(false);
    term.clear();
  }

  private waitForExit(): Promise<void> {
    return new Promise((resolve) => {
      process.on('SIGINT', () => {
        this.cleanup();
        resolve();
        process.exit(0);
      });
    });
  }
}
