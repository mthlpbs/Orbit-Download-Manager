// Rasterize the supplied Orbit SVG for the app, Windows shell and browser extensions.
// Run: npm ci --prefix tools/icon && node tools/icon/build_icon.mjs
import { readFile, writeFile } from 'node:fs/promises';
import { Resvg } from '@resvg/resvg-js';
import pngToIco from 'png-to-ico';

const root = new URL('../../', import.meta.url);
const svg = await readFile(new URL('art/Orbit Download Manager.svg', root), 'utf8');
const sizes = [16, 20, 24, 32, 48, 64, 128, 256];

// The orbit mark is wider than it is tall. Give it a full-size silhouette at taskbar sizes,
// and enlarge the artwork inside the tile so the ring and satellite stay distinct.
const windowsSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">
  <defs>
    <linearGradient id="taskbar-tile" x1="12" y1="0" x2="116" y2="128" gradientUnits="userSpaceOnUse">
      <stop stop-color="#55447C"/>
      <stop offset="1" stop-color="#2B1E48"/>
    </linearGradient>
    <clipPath id="taskbar-tile-clip"><rect x="3" y="3" width="122" height="122" rx="28"/></clipPath>
  </defs>
  <rect x="3" y="3" width="122" height="122" rx="28" fill="url(#taskbar-tile)"/>
  <g clip-path="url(#taskbar-tile-clip)">
    <g transform="translate(-8 -6.4) scale(1.1)">${svg}</g>
  </g>
  <rect x="4" y="4" width="120" height="120" rx="27" fill="none" stroke="#A894D7" stroke-opacity="0.55" stroke-width="2"/>
</svg>`;

function rasterize(artwork, size) {
  return new Resvg(artwork, { fitTo: { mode: 'width', value: size } }).render().asPng();
}

const windowsPngs = [];
for (const size of sizes) {
  const png = rasterize(svg, size);
  await writeFile(new URL(`crates/app/assets/icons/app/odm-${size}.png`, root), png);
  if ([16, 32, 48, 128].includes(size)) {
    await writeFile(new URL(`extension/icons/icon-${size}.png`, root), png);
  }
  const windowsPng = rasterize(windowsSvg, size);
  windowsPngs.push(windowsPng);
  if (size === 256) {
    await writeFile(new URL('crates/app/assets/icons/app/odm-window.png', root), windowsPng);
    await writeFile(new URL('docs/images/logo.png', root), png);
  }
}
await writeFile(new URL('crates/app/assets/icons/app/odm.ico', root), await pngToIco(windowsPngs));
await writeFile(new URL('art/Orbit Download Manager.png', root), rasterize(svg, 1024));
await import('./build_overlay_icons.mjs');
console.log('Built Orbit app, installer, browser extension and README icons.');
