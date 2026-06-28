import terminalKit from 'terminal-kit';

const term: any = terminalKit.terminal;

function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace('#', '');
  if (h.length === 3) {
    h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
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
