import terminalKit from 'terminal-kit';

const rawTerm = terminalKit.terminal;
const term: any = rawTerm || (() => {
  const fallback: any = (text: string) => process.stdout.write(text);
  fallback.width = process.stdout.columns || 80;
  fallback.height = process.stdout.rows || 24;
  fallback.clear = () => process.stdout.write('\x1b[2J\x1b[H');
  fallback.moveTo = (x: number, y: number) => process.stdout.write(`\x1b[${y};${x}H`);
  fallback.eraseLine = () => process.stdout.write('\x1b[2K');
  fallback.grabInput = () => {};
  fallback.hideCursor = () => {};
  fallback.bold = (text: string) => process.stdout.write(`\x1b[1m${text}\x1b[0m`);
  fallback.dim = (text: string) => process.stdout.write(`\x1b[2m${text}\x1b[0m`);
  fallback.on = () => {};
  return fallback;
})();

function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace('#', '');
  if (h.length === 3) {
    h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
  }
  if (!/^[0-9a-fA-F]{6}$/.test(h)) {
    return [128, 128, 128];
  }
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

term.colorHex = function (hex: string) {
  const [r, g, b] = hexToRgb(hex);
  term(`\x1b[38;2;${r};${g};${b}m`);
  return term;
};

term.bgColorHex = function (hex: string) {
  const [r, g, b] = hexToRgb(hex);
  term(`\x1b[48;2;${r};${g};${b}m`);
  return term;
};

export { term };
