import { createRequire } from 'module';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.join(__dirname, '..', 'src');
const outDir = path.join(__dirname, '..', 'dist', 'esbuild-tui-shell');

const require = createRequire(import.meta.url);
const esbuild = require('esbuild');

if (!fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

const fixTerminalKitPlugin = {
  name: 'fix-terminal-kit',
  setup(build) {
    build.onLoad({ filter: /termconfig[\\\/]README/ }, () => ({
      contents: 'module.exports = {};',
      loader: 'js',
    }));

    build.onLoad({ filter: /terminal-kit[\\\/]lib[\\\/]termkit\.js$/ }, async (args) => {
      const noLazyPath = args.path.replace('termkit.js', 'termkit-no-lazy-require.js');
      const contents = await fs.promises.readFile(noLazyPath, 'utf-8');
      return { contents, loader: 'js' };
    });
  },
};

await esbuild.build({
  entryPoints: [path.join(srcDir, 'tui', 'app.ts')],
  bundle: true,
  outfile: path.join(outDir, 'index.mjs'),
  platform: 'node',
  target: 'node20',
  format: 'esm',
  external: [],
  loader: { '': 'text' },
  plugins: [fixTerminalKitPlugin],
  minify: false,
  sourcemap: false,
  logLevel: 'info',
});

console.log(`esbuild complete: ${outDir}/index.mjs`);