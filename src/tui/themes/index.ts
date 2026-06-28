import { Theme } from '../../backend/core/types.js';

export const darkTheme: Theme = {
  name: 'dark',
  bg: '#1e1e2e',
  fg: '#cdd6f4',
  accent: '#89b4fa',
  dim: '#6c7086',
  warn: '#f9e2af',
  error: '#f38ba8',
  success: '#a6e3a1',
  border: '#45475a',
  userMsg: '#89b4fa',
  assistantMsg: '#cdd6f4',
  systemMsg: '#6c7086',
};

export const lightTheme: Theme = {
  name: 'light',
  bg: '#eff1f5',
  fg: '#4c4f69',
  accent: '#1e66f5',
  dim: '#9ca0b0',
  warn: '#df8e1d',
  error: '#d20f39',
  success: '#40a02b',
  border: '#ccd0da',
  userMsg: '#1e66f5',
  assistantMsg: '#4c4f69',
  systemMsg: '#9ca0b0',
};

export class ThemeManager {
  private themes: Map<string, Theme> = new Map();
  private currentTheme: Theme;

  constructor() {
    this.themes.set('dark', darkTheme);
    this.themes.set('light', lightTheme);
    this.currentTheme = darkTheme;
  }

  getTheme(): Theme {
    return this.currentTheme;
  }

  setTheme(name: string): void {
    const theme = this.themes.get(name);
    if (theme) {
      this.currentTheme = theme;
    }
  }

  listThemes(): string[] {
    return Array.from(this.themes.keys());
  }
}
