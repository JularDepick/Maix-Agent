import { logger } from '../core/logger.js';

export type AgentMode = 'plan' | 'agent' | 'yolo';

export interface ModeConfig {
  mode: AgentMode;
  autoApprove: boolean;
  requirePlan: boolean;
  maxToolRounds: number;
  contextStrategy: 'full' | 'compact' | 'summary';
  description: string;
}

const MODE_CONFIGS: Record<AgentMode, ModeConfig> = {
  plan: {
    mode: 'plan',
    autoApprove: false,
    requirePlan: true,
    maxToolRounds: 32,
    contextStrategy: 'full',
    description: 'Plan mode: think step by step, create plan before execution',
  },
  agent: {
    mode: 'agent',
    autoApprove: false,
    requirePlan: false,
    maxToolRounds: 16,
    contextStrategy: 'compact',
    description: 'Agent mode: standard tool use with approval',
  },
  yolo: {
    mode: 'yolo',
    autoApprove: true,
    requirePlan: false,
    maxToolRounds: 32,
    contextStrategy: 'compact',
    description: 'YOLO mode: auto-approve all tools, execute freely',
  },
};

export class ModeManager {
  private currentMode: AgentMode = 'agent';
  private modeHistory: Array<{ mode: AgentMode; timestamp: number }> = [];

  switchMode(mode: AgentMode): void {
    const previous = this.currentMode;
    this.currentMode = mode;
    this.modeHistory.push({ mode, timestamp: Date.now() });
    logger.info(`Mode switched: ${previous} -> ${mode}`);
  }

  getConfig(): ModeConfig {
    return MODE_CONFIGS[this.currentMode];
  }

  getCurrentMode(): AgentMode {
    return this.currentMode;
  }

  getAvailableModes(): AgentMode[] {
    return ['plan', 'agent', 'yolo'];
  }

  getModeDescription(mode: AgentMode): string {
    return MODE_CONFIGS[mode].description;
  }

  getModeHistory(): Array<{ mode: AgentMode; timestamp: number }> {
    return this.modeHistory;
  }

  isAutoApprove(): boolean {
    return MODE_CONFIGS[this.currentMode].autoApprove;
  }

  getMaxToolRounds(): number {
    return MODE_CONFIGS[this.currentMode].maxToolRounds;
  }
}
