// Browser regression checks for the in-page download toolbar and quality menu.
// Install Playwright and Chromium. PLAYWRIGHT_MODULE and CHROME_PATH can point to existing copies.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || "playwright");
const script = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
const messages = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
const browser = await chromium.launch(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {});
try {
  const page = await browser.newPage({ viewport: { width: 800, height: 600 }, reducedMotion: "reduce" });
  await page.addInitScript((messages) => {
    // Expose the shadow root to inspect geometry while still using trusted mouse and keyboard input.
    const attach = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function (options) {
      return attach.call(this, this.localName === "mdm-video-button" ? { ...options, mode: "open" } : options);
    };
    window.browser = {
      i18n: { getMessage: (key) => messages[key]?.message || "" },
      runtime: {
        id: "orbit-test",
        sendMessage: async ({ type }) => type === "video-lookup"
          ? new Promise((resolve) => { window.finishLookup = resolve; })
          : { ok: true, on: true, compact: true },
      },
      storage: {
        onChanged: { addListener() {} },
        local: { get: async (defaults) => ({ ...defaults, positions: window.testPositions || {} }), set: async () => {} },
      },
    };
  }, messages);
  await page.route("https://www.youtube.com/**", (route) => route.fulfill({
    contentType: "text/html",
    body: `<!doctype html><title>Video overlay fixture</title><style>
      body { margin: 0; }
      #player { display: block; position: fixed; left: 8px; top: 375px; }
      video { display: block; width: 340px; height: 180px; background: #151515; }
      #native-close { position: absolute; top: 8px; right: 4px; width: 48px; height: 48px; display: none; }
      #player:hover #native-close { display: block; }
    </style>`,
  }));

  async function load(fixture, positions = {}) {
    await page.goto("https://www.youtube.com/");
    await page.evaluate(({ fixture, positions }) => {
      window.testPositions = positions;
      document.body.innerHTML = fixture;
    }, { fixture, positions });
    await page.addScriptTag({ content: script });
  }

  const pill = page.locator("mdm-video-button .pill");
  const arrow = pill.locator(".more");
  const menu = page.locator("mdm-video-button .menu");
  const close = page.locator("#native-close");

  // A saved top-right position still leaves the YouTube close control free to click.
  const saved = { "www.youtube.com": { x: 1, y: 0 } };
  for (const fixture of [
    '<ytd-miniplayer id="player"><video></video><button id="native-close">Close</button></ytd-miniplayer>',
    '<div id="player" class="ytp-player-minimized"><video></video><button id="native-close">Close</button></div>',
  ]) {
    await load(fixture, saved);
    await page.mouse.move(180, 460);
    await pill.waitFor();
    const buttonBox = await pill.boundingBox();
    const closeBox = await close.boundingBox();
    assert.ok(buttonBox.y >= closeBox.y + closeBox.height, "toolbar sits below the miniplayer controls");
    await close.click();
  }

  // A preview can move underneath the fixed toolbar while the pointer crosses the menu.
  await load('<div id="player"><video></video><a href="/watch?v=fixture1">Video</a></div>');
  await page.mouse.move(180, 460);
  await arrow.click();
  const opening = await menu.boundingBox();
  const anchor = await arrow.boundingBox();
  assert.ok(opening.x >= 8 && opening.x + opening.width <= 792, "menu starts within the viewport");
  assert.ok(opening.y + opening.height <= anchor.y, "menu opens above a toolbar near the bottom");

  await page.locator("#player").evaluate((el) => { el.style.left = "350px"; });
  await page.mouse.move(opening.x + 12, opening.y + 40, { steps: 10 });
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.deepEqual(await menu.boundingBox(), opening, "movement over the menu cannot switch its horizontal alignment");
  assert.deepEqual(await arrow.boundingBox(), anchor, "the arrow remains stationary while the menu is open");

  // When the qualities arrive, the menu grows along the same anchor, with every edge still visible.
  await page.evaluate(() => window.finishLookup({ lookup: {
    state: "ready",
    choices: [2160, 1440, 1080, 720, 480, 360, 240, 144].map((height) => ({ height, ext: "mp4", bytes: 1024 })),
  } }));
  await menu.locator(".rows button").first().waitFor();
  const ready = await menu.boundingBox();
  assert.equal(ready.x, opening.x);
  assert.ok(ready.height > opening.height);
  assert.ok(ready.y >= 7 && Math.abs(ready.y + ready.height - opening.y - opening.height) <= 1);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  console.log("Video overlay browser checks passed: miniplayer Close, stable menu anchor and viewport bounds.");
} finally {
  await browser.close();
}
