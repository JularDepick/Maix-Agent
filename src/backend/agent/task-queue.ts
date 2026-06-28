import { v4 as uuidv4 } from 'uuid';
import { logger } from '../core/logger.js';

export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed' | 'blocked';

export interface Task {
  id: string;
  name: string;
  description: string;
  priority: number;
  dependencies: string[];
  status: TaskStatus;
  result?: unknown;
  error?: string;
  createdAt: number;
  startedAt?: number;
  completedAt?: number;
}

export class TaskQueue {
  private tasks: Map<string, Task> = new Map();
  private executionOrder: string[] = [];

  add(config: Omit<Task, 'id' | 'status' | 'createdAt'>): Task {
    const task: Task = {
      ...config,
      id: uuidv4(),
      status: 'pending',
      createdAt: Date.now(),
    };

    this.tasks.set(task.id, task);
    this.executionOrder.push(task.id);
    this.sortByPriority();
    logger.info(`Task added: ${task.name} (${task.id})`);
    return task;
  }

  remove(id: string): boolean {
    const task = this.tasks.get(id);
    if (!task) return false;

    for (const other of this.tasks.values()) {
      other.dependencies = other.dependencies.filter(d => d !== id);
    }

    this.executionOrder = this.executionOrder.filter(i => i !== id);
    this.tasks.delete(id);
    logger.info(`Task removed: ${task.name} (${id})`);
    return true;
  }

  get(id: string): Task | undefined {
    return this.tasks.get(id);
  }

  list(filter?: { status?: TaskStatus }): Task[] {
    let tasks = this.executionOrder
      .map(id => this.tasks.get(id)!)
      .filter(Boolean);

    if (filter?.status) {
      tasks = tasks.filter(t => t.status === filter.status);
    }

    return tasks;
  }

  insertAfter(afterId: string, task: Omit<Task, 'id' | 'status' | 'createdAt'>): Task {
    const newTask: Task = {
      ...task,
      id: uuidv4(),
      status: 'pending',
      createdAt: Date.now(),
    };

    this.tasks.set(newTask.id, newTask);

    const idx = this.executionOrder.indexOf(afterId);
    if (idx >= 0) {
      this.executionOrder.splice(idx + 1, 0, newTask.id);
    } else {
      this.executionOrder.push(newTask.id);
    }

    logger.info(`Task inserted after ${afterId}: ${newTask.name}`);
    return newTask;
  }

  insertBefore(beforeId: string, task: Omit<Task, 'id' | 'status' | 'createdAt'>): Task {
    const newTask: Task = {
      ...task,
      id: uuidv4(),
      status: 'pending',
      createdAt: Date.now(),
    };

    this.tasks.set(newTask.id, newTask);

    const idx = this.executionOrder.indexOf(beforeId);
    if (idx >= 0) {
      this.executionOrder.splice(idx, 0, newTask.id);
    } else {
      this.executionOrder.push(newTask.id);
    }

    logger.info(`Task inserted before ${beforeId}: ${newTask.name}`);
    return newTask;
  }

  moveToTop(id: string): void {
    const idx = this.executionOrder.indexOf(id);
    if (idx > 0) {
      this.executionOrder.splice(idx, 1);
      this.executionOrder.unshift(id);
      logger.info(`Task moved to top: ${id}`);
    }
  }

  moveToBottom(id: string): void {
    const idx = this.executionOrder.indexOf(id);
    if (idx >= 0 && idx < this.executionOrder.length - 1) {
      this.executionOrder.splice(idx, 1);
      this.executionOrder.push(id);
      logger.info(`Task moved to bottom: ${id}`);
    }
  }

  addDependency(taskId: string, dependencyId: string): void {
    const task = this.tasks.get(taskId);
    if (task && !task.dependencies.includes(dependencyId)) {
      task.dependencies.push(dependencyId);
      logger.info(`Dependency added: ${taskId} depends on ${dependencyId}`);
    }
  }

  removeDependency(taskId: string, dependencyId: string): void {
    const task = this.tasks.get(taskId);
    if (task) {
      task.dependencies = task.dependencies.filter(d => d !== dependencyId);
      logger.info(`Dependency removed: ${taskId} no longer depends on ${dependencyId}`);
    }
  }

  getReadyTasks(): Task[] {
    return this.list({ status: 'pending' }).filter(task =>
      task.dependencies.every(depId => {
        const dep = this.tasks.get(depId);
        return dep?.status === 'completed';
      })
    );
  }

  next(): Task | undefined {
    const ready = this.getReadyTasks();
    return ready[0];
  }

  start(id: string): void {
    const task = this.tasks.get(id);
    if (task && task.status === 'pending') {
      task.status = 'running';
      task.startedAt = Date.now();
      logger.info(`Task started: ${task.name} (${id})`);
    }
  }

  complete(id: string, result: unknown): void {
    const task = this.tasks.get(id);
    if (task && task.status === 'running') {
      task.status = 'completed';
      task.result = result;
      task.completedAt = Date.now();
      logger.info(`Task completed: ${task.name} (${id})`);
    }
  }

  fail(id: string, error: string): void {
    const task = this.tasks.get(id);
    if (task && task.status === 'running') {
      task.status = 'failed';
      task.error = error;
      task.completedAt = Date.now();
      logger.error(`Task failed: ${task.name} (${id}): ${error}`);
    }
  }

  private sortByPriority(): void {
    this.executionOrder.sort((a, b) => {
      const taskA = this.tasks.get(a)!;
      const taskB = this.tasks.get(b)!;
      return taskB.priority - taskA.priority;
    });
  }

  getStats(): { total: number; pending: number; running: number; completed: number; failed: number } {
    const tasks = Array.from(this.tasks.values());
    return {
      total: tasks.length,
      pending: tasks.filter(t => t.status === 'pending').length,
      running: tasks.filter(t => t.status === 'running').length,
      completed: tasks.filter(t => t.status === 'completed').length,
      failed: tasks.filter(t => t.status === 'failed').length,
    };
  }
}
