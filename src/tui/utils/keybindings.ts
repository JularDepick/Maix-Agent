export interface KeyBinding {
  key: string;
  command: string;
  description: string;
}

export const defaultKeyBindings: KeyBinding[] = [
  { key: 'CTRL_C', command: 'exit', description: 'Exit application' },
  { key: 'CTRL_L', command: 'clear', description: 'Clear screen' },
  { key: 'CTRL_U', command: 'clearInput', description: 'Clear input line' },
  { key: 'CTRL_K', command: 'clearToEnd', description: 'Clear to end of line' },
  { key: 'CTRL_A', command: 'home', description: 'Move to start of line' },
  { key: 'CTRL_E', command: 'end', description: 'Move to end of line' },
  { key: 'CTRL_W', command: 'deleteWord', description: 'Delete previous word' },
  { key: 'ENTER', command: 'submit', description: 'Submit input' },
  { key: 'BACKSPACE', command: 'backspace', description: 'Delete character' },
  { key: 'DELETE', command: 'delete', description: 'Delete forward' },
  { key: 'LEFT', command: 'cursorLeft', description: 'Move cursor left' },
  { key: 'RIGHT', command: 'cursorRight', description: 'Move cursor right' },
  { key: 'UP', command: 'historyUp', description: 'Previous history' },
  { key: 'DOWN', command: 'historyDown', description: 'Next history' },
  { key: 'TAB', command: 'autocomplete', description: 'Autocomplete' },
];

export class KeyBindingManager {
  private bindings: Map<string, KeyBinding> = new Map();

  constructor(bindings: KeyBinding[] = defaultKeyBindings) {
    for (const binding of bindings) {
      this.bindings.set(binding.key, binding);
    }
  }

  getCommand(key: string): string | undefined {
    return this.bindings.get(key)?.command;
  }

  getDescription(key: string): string | undefined {
    return this.bindings.get(key)?.description;
  }

  getKeyForCommand(command: string): string | undefined {
    for (const binding of this.bindings.values()) {
      if (binding.command === command) return binding.key;
    }
    return undefined;
  }

  listBindings(): KeyBinding[] {
    return Array.from(this.bindings.values());
  }
}
