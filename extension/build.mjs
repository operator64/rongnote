// Bundle the extension into ./dist/ as a loadable WebExtension.
// Firefox: about:debugging → "This Firefox" → "Load Temporary Add-on" → pick dist/manifest.json
// Chrome:  chrome://extensions → "Load unpacked" → pick the dist/ folder
//
// Three entrypoints — popup, options, background — bundled with esbuild.
// Static files (manifest.json, *.html, icons/) are copied verbatim.

import * as esbuild from 'esbuild';
import { cp, mkdir, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname);
const src = resolve(root, 'src');
const dist = resolve(root, 'dist');

const watch = process.argv.includes('--watch');

async function clean() {
  if (existsSync(dist)) await rm(dist, { recursive: true });
  await mkdir(dist, { recursive: true });
}

async function copyStatic() {
  // Manifest + HTMLs + icons go through unchanged.
  for (const f of ['manifest.json', 'popup.html', 'options.html']) {
    await cp(resolve(src, f), resolve(dist, f));
  }
  await cp(resolve(src, 'icons'), resolve(dist, 'icons'), { recursive: true });
}

// libsodium-wrappers-sumo's ESM build imports `./libsodium-sumo.mjs` but
// that file lives in the sibling `libsodium-sumo` package — upstream
// packaging bug. Same fix as web/vite.config.ts: rewrite the resolution.
const fixLibsodiumImport = {
  name: 'fix-libsodium-relative-import',
  setup(build) {
    build.onResolve({ filter: /^\.\/libsodium-sumo\.mjs$/ }, async (args) => {
      if (!args.importer.includes('libsodium-wrappers-sumo')) return null;
      const resolved = await build.resolve('libsodium-sumo', {
        kind: 'import-statement',
        resolveDir: args.resolveDir
      });
      return resolved;
    });
  }
};

const sharedOptions = {
  bundle: true,
  format: 'esm',
  target: ['firefox120', 'chrome120'],
  platform: 'browser',
  loader: { '.svg': 'file' },
  define: {
    'process.env.NODE_ENV': '"production"'
  },
  plugins: [fixLibsodiumImport],
  logLevel: 'info'
};

async function build() {
  await clean();
  await copyStatic();

  const ctx = await esbuild.context({
    ...sharedOptions,
    entryPoints: {
      popup: resolve(src, 'popup.ts'),
      options: resolve(src, 'options.ts'),
      background: resolve(src, 'background.ts')
    },
    outdir: dist,
    splitting: false,
    sourcemap: watch ? 'inline' : false,
    minify: !watch
  });

  if (watch) {
    await ctx.watch();
    console.log('watching for changes…');
  } else {
    await ctx.rebuild();
    await ctx.dispose();
    console.log('built →', dist);
  }
}

build().catch((err) => {
  console.error(err);
  process.exit(1);
});
