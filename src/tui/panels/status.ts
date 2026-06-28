import { ThemeManager } from '../themes/index.js';
import { term } from '../terminal.js';

export interface Metrics {
  activeAgents: number;
  totalTasks: number;
  completedTasks: number;
  totalTokens: number;
  totalCost: number;
  uptime: number;
  currentMode: string;
  taskQueueSize: number;
}

export class StatusPanel {
  private visible = false;
  private metrics: Metrics = {
    activeAgents: 0,
    totalTasks: 0,
    completedTasks: 0,
    totalTokens: 0,
    totalCost: 0,
    uptime: Date.now(),
    currentMode: 'agent',
    taskQueueSize: 0,
  };

  constructor(private theme: ThemeManager) {}

  toggle(): void {
    this.visible = !this.visible;
  }

  isVisible(): boolean {
    return this.visible;
  }

  updateMetrics(metrics: Partial<Metrics>): void {
    Object.assign(this.metrics, metrics);
  }

  render(): void {
    if (!this.visible) return;

    const theme = this.theme.getTheme();
    const startRow = 3;
    const panelWidth = 30;

    term.moveTo(term.width - panelWidth, startRow);
    term.bgColorHex(theme.border);
    term.colorHex(theme.accent);
    term.bold(' Status Panel ');
    term.eraseLine();

    const lines = [
      `Active Agents: ${this.metrics.activeAgents}`,
      `Tasks: ${this.metrics.completedTasks}/${this.metrics.totalTasks}`,
      `Queue: ${this.metrics.taskQueueSize}`,
      `Tokens: ${this.metrics.totalTokens}`,
      `Cost: $${this.metrics.totalCost.toFixed(4)}`,
      `Mode: ${this.metrics.currentMode.toUpperCase()}`,
      `Uptime: ${Math.floor((Date.now() - this.metrics.uptime) / 1000)}s`,
    ];

    for (let i = 0; i < lines.length; i++) {
      term.moveTo(term.width - panelWidth, startRow + 1 + i);
      term.bgColorHex(theme.bg);
      term.colorHex(theme.fg);
      term(` ${lines[i].padEnd(panelWidth - 2)}`);
      term.eraseLine();
    }
  }
}