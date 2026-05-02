// Render static/app-icon.svg into the PNG sizes the PWA manifest +
// apple-touch-icon conventions ask for. Run `npm run build:icons`
// after editing app-icon.svg. Output stays under static/ so SvelteKit
// serves it as-is.
//
// We intentionally don't run this on every build — the inputs change
// rarely and the outputs are committed. Keeps builds free of native
// deps for anyone who only wants to compile, not regenerate icons.

import sharp from 'sharp';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const srcSvg = path.join(here, '..', 'static', 'app-icon.svg');
const outDir = path.join(here, '..', 'static', 'icons');

await mkdir(outDir, { recursive: true });

const targets = [
  { size: 192, file: 'icon-192.png' },
  { size: 512, file: 'icon-512.png' },
  // iOS apple-touch-icon: 180×180 is the iPhone Retina size, also fine
  // for iPad. iOS rounds the corners itself.
  { size: 180, file: 'apple-touch-icon.png' }
];

for (const { size, file } of targets) {
  const out = path.join(outDir, file);
  await sharp(srcSvg).resize(size, size).png().toFile(out);
  console.log(`wrote ${path.relative(process.cwd(), out)} (${size}×${size})`);
}
