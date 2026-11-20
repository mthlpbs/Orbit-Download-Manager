// Set Orbit's date-based release version consistently. Run: node tools/release/set-version.mjs [YYYY-MM-DD]
import { readFile, writeFile, readdir } from "node:fs/promises";
const root = new URL("../../", import.meta.url);
const requested = process.argv[2] || new Date().toLocaleDateString("en-CA");
const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(requested);
const date = new Date(requested + "T12:00:00Z");
if (!match || !Number.isFinite(date.valueOf()) || date.toISOString().slice(0, 10) !== requested) throw new Error("Use a valid YYYY-MM-DD date");
// Cargo/SemVer rejects leading zeroes in numeric components; November 19, 2026 is 26.11.19.
const version = [Number(match[1]) % 100, Number(match[2]), Number(match[3])].join(".");
const edit = async (file, change) => {
  const url = new URL(file, root);
  const before = await readFile(url, "utf8");
  const after = change(before);
  if (after !== before) await writeFile(url, after);
};
await edit("Cargo.toml", (s) => s.replace(/(\[workspace\.package\][\s\S]*?\nversion\s*=\s*)"[^"]+"/, '$1"' + version + '"'));
const members = new Set();
for (const entry of await readdir(new URL("crates/", root), { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  try {
    const manifest = await readFile(new URL("crates/" + entry.name + "/Cargo.toml", root), "utf8");
    if (/version\.workspace\s*=\s*true/.test(manifest)) members.add(/^name\s*=\s*"([^"]+)"/m.exec(manifest)[1]);
  } catch (error) { if (error.code !== "ENOENT") throw error; }
}
await edit("Cargo.lock", (s) => s.replace(/\[\[package\]\][\s\S]*?(?=\[\[package\]\]|$)/g, (block) => {
  const name = /^name = "([^"]+)"/m.exec(block)?.[1];
  return members.has(name) && !/^source = /m.test(block) ? block.replace(/^version = "[^"]+"/m, 'version = "' + version + '"') : block;
}));
for (const file of ["extension/manifest.json", "extension/manifest.firefox.json"]) {
  await edit(file, (s) => s.replace(/("version"\s*:\s*)"[^"]+"/, '$1"' + version + '"'));
}
await edit("tools/installer/odm.iss", (s) => s.replace(/(#define AppVersion\s+)"[^"]+"/, '$1"' + version + '"'));
console.log("Orbit Download Manager " + version);
