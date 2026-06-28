import { BaseProvider } from './base.js';
import { logger } from '../core/logger.js';

export type TaskType = 'code' | 'reasoning' | 'creative' | 'analysis' | 'general';
export type TaskComplexity = 'simple' | 'medium' | 'complex';

export interface TaskClassification {
  type: TaskType;
  complexity: TaskComplexity;
  keywords: string[];
}

export interface ModelRoute {
  taskType: TaskType;
  providerName: string;
  priority: number;
}

const CODE_KEYWORDS = [
  'code', 'function', 'class', 'method', 'bug', 'debug', 'refactor',
  'implement', 'fix', 'error', 'compile', 'build', 'test', 'deploy',
  'typescript', 'javascript', 'python', 'rust', 'java', 'api',
];

const REASONING_KEYWORDS = [
  'why', 'how', 'explain', 'analyze', 'reason', 'logic', 'prove',
  'argue', 'debate', 'evaluate', 'compare', 'contrast', 'assess',
];

const CREATIVE_KEYWORDS = [
  'write', 'story', 'poem', 'creative', 'imagine', 'design', 'art',
  'brainstorm', 'idea', 'concept', 'narrative', 'describe',
];

const ANALYSIS_KEYWORDS = [
  'data', 'statistics', 'chart', 'graph', 'trend', 'pattern',
  'analyze', 'measure', 'metrics', 'performance', 'optimize',
];

export class TaskClassifier {
  classify(task: string): TaskClassification {
    const lower = task.toLowerCase();
    const words = lower.split(/\s+/);

    const scores: Record<TaskType, number> = {
      code: 0,
      reasoning: 0,
      creative: 0,
      analysis: 0,
      general: 0,
    };

    for (const word of words) {
      if (CODE_KEYWORDS.some(k => word.includes(k))) scores.code++;
      if (REASONING_KEYWORDS.some(k => word.includes(k))) scores.reasoning++;
      if (CREATIVE_KEYWORDS.some(k => word.includes(k))) scores.creative++;
      if (ANALYSIS_KEYWORDS.some(k => word.includes(k))) scores.analysis++;
    }

    const maxScore = Math.max(...Object.values(scores));
    let type: TaskType = 'general';

    if (maxScore > 0) {
      type = Object.entries(scores).find(([, s]) => s === maxScore)?.[0] as TaskType || 'general';
    }

    let complexity: TaskComplexity = 'simple';
    if (task.length > 200 || words.length > 50) {
      complexity = 'complex';
    } else if (task.length > 50 || words.length > 15) {
      complexity = 'medium';
    }

    const matchedKeywords = words.filter(w =>
      [...CODE_KEYWORDS, ...REASONING_KEYWORDS, ...CREATIVE_KEYWORDS, ...ANALYSIS_KEYWORDS]
        .some(k => w.includes(k))
    );

    return { type, complexity, keywords: matchedKeywords };
  }
}

export class ModelRouter {
  private routes: ModelRoute[] = [];
  private classifier: TaskClassifier;
  private defaultProvider: string;

  constructor(defaultProvider: string) {
    this.defaultProvider = defaultProvider;
    this.classifier = new TaskClassifier();
    this.setupDefaultRoutes();
  }

  private setupDefaultRoutes(): void {
    this.routes = [
      { taskType: 'code', providerName: 'openai', priority: 1 },
      { taskType: 'reasoning', providerName: 'anthropic', priority: 1 },
      { taskType: 'creative', providerName: 'anthropic', priority: 1 },
      { taskType: 'analysis', providerName: 'openai', priority: 1 },
      { taskType: 'general', providerName: this.defaultProvider, priority: 1 },
    ];
  }

  route(task: string): { classification: TaskClassification; providerName: string } {
    const classification = this.classifier.classify(task);
    const route = this.routes
      .filter(r => r.taskType === classification.type)
      .sort((a, b) => a.priority - b.priority)[0];

    const providerName = route?.providerName || this.defaultProvider;
    logger.debug(`Router: task type=${classification.type}, complexity=${classification.complexity}, provider=${providerName}`);

    return { classification, providerName };
  }

  addRoute(route: ModelRoute): void {
    this.routes.push(route);
    logger.info(`Route added: ${route.taskType} -> ${route.providerName}`);
  }

  removeRoute(taskType: string): void {
    this.routes = this.routes.filter(r => r.taskType !== taskType);
    logger.info(`Route removed: ${taskType}`);
  }

  getRoutes(): ModelRoute[] {
    return [...this.routes];
  }

  setDefaultProvider(provider: string): void {
    this.defaultProvider = provider;
  }
}
