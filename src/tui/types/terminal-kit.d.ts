declare module 'terminal-kit' {
  interface Terminal {
    (text: string): void;
    moveTo(x: number, y: number): void;
    eraseLine(): void;
    clear(): void;
    grabInput(grab: boolean): void;
    hideCursor(hide: boolean): void;
    bold(text: string): void;
    dim(text: string): void;
    colorHex(hex: string): void;
    bgColorHex(hex: string): void;
    bgColor(color: number): void;
    inputField(options: Record<string, unknown>, callback: (error: Error | null, input: string | undefined) => void): void;
    on(event: 'key', handler: (name: string) => void): void;
    on(event: string, handler: (...args: unknown[]) => void): void;
    width: number;
    height: number;
  }

  const terminal: Terminal;
  export default { terminal };
}
