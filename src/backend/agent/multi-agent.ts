import { v4 as uuidv4 } from 'uuid';
import { Agent, AgentConfig } from './agent.js';
import { AgentEvent, Message } from '../core/types.js';
import { EventBus } from '../core/event-bus.js';
import { TaskQueue, Task } from './task-queue.js';
import { logger } from '../core/logger.js';

export type CollaborationMode = 'hierarchical' | 'collaborative' | 'debate';

export interface AgentStatus {
  agentId: string;
  role: string;
  status: 'idle' | 'busy' | 'error';
  currentTask?: string;
  completedTasks: number;
  totalTokens: number;
}

export class MultiAgentCoordinator {
  private agents: Map<string, Agent> = new Map();
  private agentStatuses: Map<string, AgentStatus> = new Map();
  private eventBus: EventBus;
  private taskQueue: TaskQueue;
  private masterAgentId: string | null = null;

  constructor(eventBus: EventBus) {
    this.eventBus = eventBus;
    this.taskQueue = new TaskQueue();
  }

  createHierarchy(masterConfig: AgentConfig, workerConfigs: AgentConfig[]): string {
    const masterAgent = new Agent(masterConfig);
    const masterId = uuidv4();
    masterAgent.setId(masterId);
    this.agents.set(masterId, masterAgent);
    this.masterAgentId = masterId;

    this.agentStatuses.set(masterId, {
      agentId: masterId,
      role: 'master',
      status: 'idle',
      completedTasks: 0,
      totalTokens: 0,
    });

    for (const config of workerConfigs) {
      const workerAgent = new Agent(config);
      const workerId = uuidv4();
      workerAgent.setId(workerId);
      this.agents.set(workerId, workerAgent);

      this.agentStatuses.set(workerId, {
        agentId: workerId,
        role: 'worker',
        status: 'idle',
        completedTasks: 0,
        totalTokens: 0,
      });
    }

    logger.info(`Hierarchy created: master=${masterId}, workers=${workerConfigs.length}`);
    return masterId;
  }

  collaborate(task: string, mode: CollaborationMode): AsyncGenerator<AgentEvent> {
    switch (mode) {
      case 'hierarchical':
        return this.hierarchicalCollaborate(task);
      case 'collaborative':
        return this.collaborativeCollaborate(task);
      case 'debate':
        return this.debateCollaborate(task);
    }
  }

  private async *hierarchicalCollaborate(task: string): AsyncGenerator<AgentEvent> {
    const masterId = this.masterAgentId;
    if (!masterId) {
      yield { type: 'error', error: 'No master agent configured' };
      return;
    }

    const master = this.agents.get(masterId);
    if (!master) {
      yield { type: 'error', error: 'Master agent not found' };
      return;
    }

    this.updateStatus(masterId, 'busy', 'Analyzing and decomposing task');

    const decompositionPrompt = `You are the master agent. Decompose this task into subtasks for worker agents.

Task: ${task}

Respond with a JSON array of subtasks:
[
  {"name": "subtask name", "description": "detailed description", "priority": 1-10}
]`;

    let decompositionResult = '';
    for await (const event of master.run(decompositionPrompt)) {
      if (event.type === 'text' && event.content) {
        decompositionResult += event.content;
        yield event;
      }
    }

    let subtasks: Array<{ name: string; description: string; priority: number }> = [];
    try {
      const jsonMatch = decompositionResult.match(/\[[\s\S]*\]/);
      if (jsonMatch) {
        subtasks = JSON.parse(jsonMatch[0]);
      }
    } catch (error) {
      logger.error('Failed to parse decomposition:', error);
      subtasks = [{ name: 'original_task', description: task, priority: 5 }];
    }

    yield { type: 'text', content: `\n[Master decomposed into ${subtasks.length} subtasks]\n` };

    const workerIds = Array.from(this.agents.keys()).filter(id => id !== masterId);

    for (let i = 0; i < subtasks.length; i++) {
      const subtask = subtasks[i];
      const workerId = workerIds[i % workerIds.length];
      const worker = this.agents.get(workerId);

      if (!worker) continue;

      this.updateStatus(workerId, 'busy', subtask.name);
      this.taskQueue.add({
        name: subtask.name,
        description: subtask.description,
        priority: subtask.priority,
        dependencies: [],
      });

      yield { type: 'text', content: `\n[Worker ${i + 1}: ${subtask.name}]\n` };

      for await (const event of worker.run(subtask.description)) {
        yield event;
      }

      this.updateStatus(workerId, 'idle');
      this.taskQueue.list().forEach(t => {
        if (t.name === subtask.name) this.taskQueue.complete(t.id, 'done');
      });
    }

    const synthesisPrompt = `You are the master agent. Synthesize the results from all workers into a final response.

Original task: ${task}

Provide a comprehensive final answer.`;

    yield { type: 'text', content: '\n[Master synthesizing results]\n' };

    for await (const event of master.run(synthesisPrompt)) {
      yield event;
    }

    this.updateStatus(masterId, 'idle');
    yield { type: 'done' };
  }

  private async *collaborativeCollaborate(task: string): AsyncGenerator<AgentEvent> {
    const agentIds = Array.from(this.agents.keys());
    if (agentIds.length === 0) {
      yield { type: 'error', error: 'No agents configured' };
      return;
    }

    yield { type: 'text', content: `[Collaborative mode: ${agentIds.length} agents working together]\n\n` };

    const results: Map<string, string> = new Map();

    for (const agentId of agentIds) {
      const agent = this.agents.get(agentId);
      if (!agent) continue;

      this.updateStatus(agentId, 'busy', 'Contributing to task');

      const agentPrompt = `You are one of several agents collaborating on this task. Provide your unique perspective and contribution.

Task: ${task}

Previous contributions:
${Array.from(results.entries()).map(([id, r]) => `Agent ${id.slice(0, 8)}: ${r.slice(0, 200)}`).join('\n') || 'None yet'}

Provide your contribution:`;

      let contribution = '';
      for await (const event of agent.run(agentPrompt)) {
        if (event.type === 'text' && event.content) {
          contribution += event.content;
          yield event;
        }
      }

      results.set(agentId, contribution);
      this.updateStatus(agentId, 'idle');
    }

    yield { type: 'text', content: '\n[All agents have contributed]\n' };
    yield { type: 'done' };
  }

  private async *debateCollaborate(task: string): AsyncGenerator<AgentEvent> {
    const agentIds = Array.from(this.agents.keys());
    if (agentIds.length < 2) {
      yield { type: 'error', error: 'Debate requires at least 2 agents' };
      return;
    }

    yield { type: 'text', content: `[Debate mode: ${agentIds.length} agents debating]\n\n` };

    const rounds = 3;
    const argumentsPerRound: Map<string, string[]> = new Map();

    for (let round = 0; round < rounds; round++) {
      yield { type: 'text', content: `\n--- Round ${round + 1} ---\n` };

      for (const agentId of agentIds) {
        const agent = this.agents.get(agentId);
        if (!agent) continue;

        this.updateStatus(agentId, 'busy', `Debating round ${round + 1}`);

        const previousArguments: string[] = [];
        for (const [id, args] of argumentsPerRound) {
          if (args.length > round) {
            previousArguments.push(`Agent ${id.slice(0, 8)}: ${args[round]}`);
          }
        }

        const debatePrompt = `You are debating on this topic. This is round ${round + 1} of ${rounds}.

Topic: ${task}

Previous arguments:
${previousArguments.join('\n') || 'No previous arguments'}

Provide your argument (be concise, max 200 words):`;

        let argument = '';
        for await (const event of agent.run(debatePrompt)) {
          if (event.type === 'text' && event.content) {
            argument += event.content;
            yield event;
          }
        }

        const agentArgs = argumentsPerRound.get(agentId) || [];
        agentArgs.push(argument);
        argumentsPerRound.set(agentId, agentArgs);

        this.updateStatus(agentId, 'idle');
      }
    }

    const judgeAgent = this.agents.get(agentIds[0]);
    if (judgeAgent) {
      yield { type: 'text', content: '\n--- Final Judgment ---\n' };

      const allArguments: string[] = [];
      for (const [id, args] of argumentsPerRound) {
        allArguments.push(`Agent ${id.slice(0, 8)}:\n${args.join('\n')}`);
      }

      const judgmentPrompt = `You are the judge. Review all arguments and provide a final conclusion.

Topic: ${task}

Arguments:
${allArguments.join('\n\n')}

Provide your final judgment:`;

      for await (const event of judgeAgent.run(judgmentPrompt)) {
        yield event;
      }
    }

    yield { type: 'done' };
  }

  private updateStatus(agentId: string, status: AgentStatus['status'], currentTask?: string): void {
    const agentStatus = this.agentStatuses.get(agentId);
    if (agentStatus) {
      agentStatus.status = status;
      agentStatus.currentTask = currentTask;
    }
  }

  getAgentStatus(agentId: string): AgentStatus | undefined {
    return this.agentStatuses.get(agentId);
  }

  getAllStatuses(): AgentStatus[] {
    return Array.from(this.agentStatuses.values());
  }

  getTaskQueue(): TaskQueue {
    return this.taskQueue;
  }
}
