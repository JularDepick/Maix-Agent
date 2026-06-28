import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.join(__dirname, '..', 'src');
const outDir = path.join(__dirname, '..', 'build', 'bunbuild-tui-with-backend-windows_x64');
const esbuildScript = path.join(__dirname, 'esbuild-tui-with-backend.mjs');

if (!fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

console.log('Step 1: Bundling with esbuild...');
execSync(`node ${esbuildScript}`, { cwd: srcDir, stdio: 'inherit' });

const bundlePath = path.join(__dirname, '..', 'dist', 'esbuild-tui-with-backend', 'index.mjs');
if (!fs.existsSync(bundlePath)) {
  console.error('Bundle not found after esbuild step');
  process.exit(1);
}

console.log('\nStep 2: Compiling with Bun...');

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

const wasmSrc = path.join(srcDir, 'node_modules', 'sql.js', 'dist', 'sql-wasm.wasm');
if (fs.existsSync(wasmSrc)) {
  fs.copyFileSync(wasmSrc, path.join(outDir, 'sql-wasm.wasm'));
  console.log('Copied sql-wasm.wasm');
}

const envSrc = path.join(srcDir, '.env.example');
const envDest = path.join(outDir, '.env');
if (fs.existsSync(envSrc)) {
  fs.copyFileSync(envSrc, envDest);
  console.log('Copied .env.example -> .env');
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