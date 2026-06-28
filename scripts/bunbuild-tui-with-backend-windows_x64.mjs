import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(__dirname, '..');
const srcDir = path.join(rootDir, 'src');
const outDir = path.join(rootDir, 'build', 'bunbuild-tui-with-backend-windows_x64');
const esbuildScript = path.join(__dirname, 'esbuild-tui-with-backend.mjs');

if (!fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

console.log('Step 1: Embedding sql-wasm.wasm...');
const wasmSrc = path.join(rootDir, 'node_modules', 'sql.js', 'dist', 'sql-wasm.wasm');
const wasmOutPath = path.join(srcDir, 'backend', 'agent', 'sql-wasm-embedded.ts');

if (fs.existsSync(wasmSrc)) {
  const wasmBuffer = fs.readFileSync(wasmSrc);
  const base64 = wasmBuffer.toString('base64');
  const content = `// Auto-generated - DO NOT EDIT
// Embedded sql-wasm.wasm (${(wasmBuffer.length / 1024 / 1024).toFixed(1)} MB)

export const SQL_WASM_BASE64 = '${base64}';

export function getSqlWasmBuffer(): Buffer {
  return Buffer.from(SQL_WASM_BASE64, 'base64');
}
`;
  fs.writeFileSync(wasmOutPath, content);
  console.log(`✓ Embedded sql-wasm.wasm (${(wasmBuffer.length / 1024 / 1024).toFixed(1)} MB)\n`);
} else {
  console.warn('Warning: sql-wasm.wasm not found, skipping embedding\n');
}

console.log('Step 2: Bundling with esbuild...');
execSync(`node ${esbuildScript}`, { cwd: srcDir, stdio: 'inherit' });

const bundlePath = path.join(rootDir, 'dist', 'esbuild-tui-with-backend', 'index.mjs');
if (!fs.existsSync(bundlePath)) {
  console.error('Bundle not found after esbuild step');
  process.exit(1);
}

console.log('\nStep 3: Compiling with Bun...');

const targets = [
  { name: 'maix-agent-all-win-x64.exe', target: 'bun-windows-x64' },
];

for (const { name, target } of targets) {
  console.log(`Building for ${target}...`);
  try {
    execSync(
      `bun build --compile --target=${target} --outfile=${path.join(outDir, name)} ${bundlePath}`,
      { cwd: srcDir, stdio: 'inherit' }
    );
    console.log(`✓ ${name}\n`);
  } catch (error) {
    console.error(`✗ Failed to build ${name}:`, error.message);
    process.exit(1);
  }
}

const envSrc = path.join(rootDir, '.env.example');
const envDest = path.join(outDir, '.env');
if (fs.existsSync(envSrc)) {
  fs.copyFileSync(envSrc, envDest);
  console.log('Copied .env.example -> .env');
} else {
  console.warn('Warning: .env.example not found at', envSrc);
}

console.log('\nBuild complete!');
console.log(`Output directory: ${outDir}`);
console.log('\nFiles:');
const files = fs.readdirSync(outDir).filter(f => !fs.statSync(path.join(outDir, f)).isDirectory());
for (const file of files) {
  const stats = fs.statSync(path.join(outDir, file));
  console.log(`  ${file}  ${(stats.size / 1024 / 1024).toFixed(1)} MB`);
}
console.log('\nUsage:');
console.log('  maix-agent-all-win-x64.exe');
