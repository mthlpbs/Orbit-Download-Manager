// Rasterize the supplied Orbit SVG faithfully for Windows, the app and browser extensions.
// Run: npm ci --prefix tools/icon && node tools/icon/build_icon.mjs
import { readFile, writeFile } from 'node:fs/promises';
import { Resvg } from '@resvg/resvg-js';
import pngToIco from 'png-to-ico';

const root = new URL('../../', import.meta.url);
const svg = await readFile(new URL('art/Orbit Download Manager.svg', root), 'utf8');
const sizes = [16, 20, 24, 32, 48, 64, 128, 256];
const pngs = new Map();
for (const size of sizes) {
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
  pngs.set(size, png);
  await writeFile(new URL(`crates/app/assets/icons/app/odm-${size}.png`, root), png);
  if ([16, 32, 48, 128].includes(size)) {
    await writeFile(new URL(`extension/icons/icon-${size}.png`, root), png);
  }
}
await writeFile(new URL('crates/app/assets/icons/app/odm.ico', root), await pngToIco([...pngs.values()]));
await writeFile(new URL('docs/images/logo.png', root), pngs.get(256));
await writeFile(new URL('art/Orbit Download Manager.png', root),
  new Resvg(svg, { fitTo: { mode: 'width', value: 1024 } }).render().asPng());
await import('./build_overlay_icons.mjs');
console.log('Built Orbit app, installer, browser extension and README icons.');
