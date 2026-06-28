import fs from 'fs/promises';
import { logger } from '../core/logger.js';
import { Agent, AgentConfig } from './agent.js';
import { AgentEvent } from '../core/types.js';
import { EventBus } from '../core/event-bus.js';

export interface TopologyNode {
  id: string;
  type: 'agent' | 'tool' | 'condition' | 'loop';
  config: Record<string, unknown>;
}

export interface TopologyEdge {
  from: string;
  to: string;
  condition?: string;
}

export interface Topology {
  name: string;
  description: string;
  nodes: TopologyNode[];
  edges: TopologyEdge[];
}

export const BUILTIN_TOPOLOGIES: Record<string, string> = {
  sequential: 'Sequential execution: tasks run one after another',
  parallel: 'Parallel execution: tasks run concurrently',
  pipeline: 'Pipeline: output of one task feeds into the next',
};

export class TopologyParser {
  parseToml(content: string): Topology {
    const topology: Topology = {
      name: '',
      description: '',
      nodes: [],
      edges: [],
    };

    let currentSection = '';
    let currentNode: Partial<TopologyNode> | null = null;

    for (const line of content.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;

      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        currentSection = trimmed.slice(1, -1);
        if (currentSection.startsWith('node.')) {
          currentNode = { id: currentSection.slice(5) };
        } else {
          currentNode = null;
        }
        continue;
      }

      const eqIdx = trimmed.indexOf('=');
      if (eqIdx < 0) continue;

      const key = trimmed.slice(0, eqIdx).trim();
      let value = trimmed.slice(eqIdx + 1).trim();

      if ((value.startsWith('"') && value.endsWith('"')) ||
          (value.startsWith("'") && value.endsWith("'"))) {
        value = value.slice(1, -1);
      }

      if (currentSection === 'topology') {
        switch (key) {
          case 'name': topology.name = value; break;
          case 'description': topology.description = value; break;
        }
      } else if (currentNode) {
        if (!currentNode.config) currentNode.config = {};
        currentNode.config[key] = value;
      } else if (currentSection === 'edge') {
        const edgeParts = value.split('->').map(s => s.trim());
        if (edgeParts.length >= 2) {
          topology.edges.push({
            from: edgeParts[0],
            to: edgeParts[1],
            condition: edgeParts[2] || undefined,
          });
        }
      }
    }

    if (currentNode && currentNode.id) {
      topology.nodes.push(currentNode as TopologyNode);
    }

    return topology;
  }

  parseJson(content: string): Topology {
    return JSON.parse(content);
  }
}

export class TopologyExecutor {
  private topology: Topology | null = null;
  private agents: Map<string, Agent> = new Map();
  private eventBus: EventBus;

  constructor(eventBus: EventBus) {
    this.eventBus = eventBus;
  }

  load(content: string, format: 'toml' | 'json' = 'toml'): void {
    const parser = new TopologyParser();
    this.topology = format === 'toml'
      ? parser.parseToml(content)
      : parser.parseJson(content);
    logger.info(`Topology loaded: ${this.topology.name}`);
  }

  registerAgent(nodeId: string, agent: Agent): void {
    this.agents.set(nodeId, agent);
  }

  async *execute(input: string): AsyncGenerator<AgentEvent> {
    if (!this.topology) {
      yield { type: 'error', error: 'No topology loaded' };
      return;
    }

    logger.info(`Executing topology: ${this.topology.name}`);

    const adjacency = this.buildAdjacency();
    const inDegree = this.buildInDegree();
    const executionOrder = this.topologicalSort(inDegree);

    const results: Map<string, string> = new Map();
    results.set('input', input);

    for (const nodeId of executionOrder) {
      const node = this.topology.nodes.find(n => n.id === nodeId);
      if (!node) continue;

      const agent = this.agents.get(nodeId);
      if (!agent) {
        logger.warn(`No agent registered for node: ${nodeId}`);
        continue;
      }

      const nodeInput = this.resolveInput(nodeId, results, adjacency);
      logger.info(`Executing node: ${nodeId}`);

      for await (const event of agent.run(nodeInput)) {
        yield event;

        if (event.type === 'done') {
          break;
        }

        if (event.type === 'text' && event.content) {
          const currentResult = results.get(nodeId) || '';
          results.set(nodeId, currentResult + event.content);
        }
      }

      this.eventBus.emit('agent:done', { agentId: nodeId });
    }

    const finalResult = results.get(executionOrder[executionOrder.length - 1]) || '';
    yield { type: 'text', content: `\n\n[Topology ${this.topology.name} completed]` };
    yield { type: 'done' };
  }

  private buildAdjacency(): Map<string, string[]> {
    const adj = new Map<string, string[]>();
    if (!this.topology) return adj;

    for (const node of this.topology.nodes) {
      adj.set(node.id, []);
    }

    for (const edge of this.topology.edges) {
      const targets = adj.get(edge.from) || [];
      targets.push(edge.to);
      adj.set(edge.from, targets);
    }

    return adj;
  }

  private buildInDegree(): Map<string, number> {
    const inDegree = new Map<string, number>();
    if (!this.topology) return inDegree;

    for (const node of this.topology.nodes) {
      inDegree.set(node.id, 0);
    }

    for (const edge of this.topology.edges) {
      inDegree.set(edge.to, (inDegree.get(edge.to) || 0) + 1);
    }

    return inDegree;
  }

  private topologicalSort(inDegree: Map<string, number>): string[] {
    const queue: string[] = [];
    const result: string[] = [];

    for (const [node, degree] of inDegree) {
      if (degree === 0) queue.push(node);
    }

    while (queue.length > 0) {
      const node = queue.shift()!;
      result.push(node);

      const targets = this.buildAdjacency().get(node) || [];
      for (const target of targets) {
        const newDegree = (inDegree.get(target) || 1) - 1;
        inDegree.set(target, newDegree);
        if (newDegree === 0) queue.push(target);
      }
    }

    return result;
  }

  private resolveInput(nodeId: string, results: Map<string, string>, adjacency: Map<string, string[]>): string {
    const inputs: string[] = [];

    for (const [from, targets] of adjacency) {
      if (targets.includes(nodeId)) {
        const result = results.get(from);
        if (result) inputs.push(result);
      }
    }

    return inputs.length > 0 ? inputs.join('\n\n') : results.get('input') || '';
  }

  getTopology(): Topology | null {
    return this.topology;
  }
}
