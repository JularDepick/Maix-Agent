export function parseMarkdown(text: string): string {
  let result = text;

  result = result.replace(/^### (.+)$/gm, '\x1b[1;36m$1\x1b[0m');
  result = result.replace(/^## (.+)$/gm, '\x1b[1;35m$1\x1b[0m');
  result = result.replace(/^# (.+)$/gm, '\x1b[1;34m$1\x1b[0m');

  result = result.replace(/\*\*(.+?)\*\*/g, '\x1b[1m$1\x1b[0m');
  result = result.replace(/\*(.+?)\*/g, '\x1b[3m$1\x1b[0m');
  result = result.replace(/`([^`]+)`/g, '\x1b[33m$1\x1b[0m');

  result = result.replace(/^[-*] (.+)$/gm, '  • $1');

  result = result.replace(/^(\d+)\. (.+)$/gm, '  $1. $2');

  result = result.replace(/^> (.+)$/gm, '\x1b[90m│ $1\x1b[0m');

  result = result.replace(/\[(.+?)\]\((.+?)\)/g, '\x1b[4;34m$1\x1b[0m');

  return result;
}

export function wrapText(text: string, maxWidth: number): string[] {
  const lines: string[] = [];
  const paragraphs = text.split('\n');

  for (const paragraph of paragraphs) {
    if (paragraph.length === 0) {
      lines.push('');
      continue;
    }

    const words = paragraph.split(' ');
    let currentLine = '';

    for (const word of words) {
      if (currentLine.length + word.length + 1 > maxWidth) {
        if (currentLine) lines.push(currentLine);
        currentLine = word;
      } else {
        currentLine += (currentLine ? ' ' : '') + word;
      }
    }

    if (currentLine) lines.push(currentLine);
  }

  return lines;
}

export function stripAnsi(text: string): string {
  return text.replace(/\x1b\[[0-9;]*m/g, '');
}

export function getVisibleLength(text: string): number {
  return stripAnsi(text).length;
}
