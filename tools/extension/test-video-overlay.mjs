// Browser regression checks for the in-page download toolbar and quality menu.
// Install Playwright and Chromium. PLAYWRIGHT_MODULE and CHROME_PATH can point to existing copies.
import assert from "node:assert/strict";
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || "playwright");
const script = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
const messages = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
const choices = [
  ...[2160, 1440, 1080, 720, 480, 360, 240, 144].map((height) => ({ height, ext: "mp4", size: Math.round(height * (1 << 20) / 10) })),
  { audio: true, original: true, ext: "webm", kbps: 133, size: 2_700_000 },
  { audio: true, ext: "m4a", kbps: 128, size: 2_600_000 },
];
const browser = await chromium.launch(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {});
try {
  const page = await browser.newPage({ viewport: { width: 800, height: 600 }, reducedMotion: "reduce" });
  await page.addInitScript((messages) => {
    // Expose the shadow root to inspect geometry while still using trusted mouse and keyboard input.
    const attach = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function (options) {
      return attach.call(this, this.localName === "odm-video-button" ? { ...options, mode: "open" } : options);
    };
    window.browser = {
      i18n: { getMessage: (key, subs = []) => {
        const entry = messages[key];
        return entry?.message.replace(/\$([a-z]+)\$/g, (_, name) => {
          const slot = entry.placeholders?.[name]?.content;
          return slot ? subs[Number(slot.slice(1)) - 1] : "";
        }) || "";
      } },
      runtime: {
        id: "orbit-test",
        sendMessage: async (message) => {
          window.orbitMessages.push(message);
          if (message.type === "video-lookup") {
            if (window.testLookup) return { lookup: window.testLookup };
            return new Promise((resolve) => { window.finishLookup = resolve; });
          }
          if (message.type === "video-pick") {
            return new Promise((resolve) => { window.finishPick = resolve; });
          }
          return { ok: true, on: true, compact: Boolean(window.testCompact) };
        },
      },
      storage: {
        onChanged: { addListener() {} },
        local: { get: async (defaults) => ({ ...defaults, positions: window.testPositions || {} }), set: async () => {} },
      },
    };
    window.orbitMessages = [];
    window.playerKeys = [];
    window.blockedAssets = [];
    window.addEventListener("keydown", (event) => window.playerKeys.push(event.key));
    window.addEventListener("securitypolicyviolation", (event) => window.blockedAssets.push(event.blockedURI));
  }, messages);
  await page.route(/^https:\/\/(?:www\.youtube\.com|www\.instagram\.com|x\.com|social\.example\.test)\//, (route) => route.fulfill({
    contentType: "text/html",
    headers: { "Content-Security-Policy": "img-src 'none'; font-src 'none'" },
    body: `<!doctype html><title>Video overlay fixture</title><style>
      body { margin: 0; }
      #player { display: block; position: fixed; left: 8px; top: 375px; }
      video { display: block; width: 340px; height: 180px; background: #151515; }
      #native-close { position: absolute; top: 8px; right: 4px; width: 48px; height: 48px; display: none; }
      #player:hover #native-close { display: block; }
    </style>`,
  }));
  async function load(fixture, positions = {}, compact = false, url = "https://www.youtube.com/", lookup = null) {
    await page.goto(url);
    await page.evaluate(({ fixture, positions, compact, lookup }) => {
      window.testPositions = positions;
      window.testCompact = compact;
      window.testLookup = lookup;
      document.body.innerHTML = fixture;
    }, { fixture, positions, compact, lookup });
    await page.addScriptTag({ content: script });
  }

  const pill = page.locator("odm-video-button .pill");
  const arrow = pill.locator(".more");
  const menu = page.locator("odm-video-button .menu");
  const close = page.locator("#native-close");
  const nextPaint = () => page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));

  for (const { feed, link, post, file, lookup } of [
    { feed: "https://www.instagram.com/", link: "/reel/FeedReel1234/?igsh=tracking",
      post: "https://www.instagram.com/reel/FeedReel1234/", file: "https://media.example.test/reel.mp4?sig=fixture",
      lookup: { state: "list" } },
    { feed: "https://x.com/home", link: "/somebody/status/1850000000000000001?s=20",
      post: "https://x.com/somebody/status/1850000000000000001", lookup: { state: "failed" } },
    { feed: "https://social.example.test/home", link: "/@author/113000000000000001",
      post: "https://social.example.test/@author/113000000000000001", lookup: { state: "failed" } },
  ]) {
    await load('<article><a href="' + link + '"><time>1d</time></a>' +
      '<div id="player" style="top:90px;cursor:pointer"><video></video>' +
      '<button aria-label="Hidden control" style="position:absolute;right:4px;top:4px;width:45px;height:45px;opacity:0"></button>' +
      '</div></article>', {}, false, feed, lookup);
    if (file) await page.locator("video").evaluate((v, file) => Object.defineProperty(v, "currentSrc", { value: file }), file);
    await page.mouse.move(180, 150);
    await pill.waitFor();
    const [button, player] = await Promise.all([pill.boundingBox(), page.locator("video").boundingBox()]);
    assert.ok(player.x + player.width - (button.x + button.width) <= 12 && button.y - player.y <= 12,
      feed + " uses the player's top-right corner");
    await pill.locator(".go").click();
    await page.waitForFunction(() => window.orbitMessages.some((m) => m.type === "video"));
    const { asked, sent } = await page.evaluate(() => ({
      asked: window.orbitMessages.find((m) => m.type === "video-lookup"),
      sent: window.orbitMessages.find((m) => m.type === "video"),
    }));
    assert.equal(asked.url, post);
    assert.equal(asked.src, "", "look up the post before falling back to its media");
    assert.equal(sent.url, post);
    assert.equal(sent.src, file || "");
    assert.equal(Boolean(sent.direct), Boolean(file));
    assert.equal(page.url(), feed, "downloading from the feed does not navigate to the post");
    await page.mouse.move(400, 150);
  }

  // A current YouTube feed card can have only a thumbnail. The watch link is still usable,
  // even when YouTube has kept a different, hidden player around.
  const card = (href) => `<yt-lockup-view-model id="player" style="top:90px">
    <a class="yt-lockup-view-model-wiz__content-image" style="display:block;width:340px;height:180px;background:#303030" href="${href}">
      <yt-thumbnail-view-model style="display:block;height:100%"></yt-thumbnail-view-model>
    </a>
    <a class="yt-lockup-metadata-view-model-wiz__title" href="${href}">Feed video title</a>
  </yt-lockup-view-model>`;
  await load(card("/watch?v=TestVideo01&pp=tracking") + '<div id="movie_player" video-id="OtherVideo1" hidden><video></video></div>');
  await page.mouse.move(180, 150);
  await pill.waitFor();
  await pill.locator(".go").hover();
  await arrow.click();
  await menu.waitFor();
  assert.equal(await menu.locator(".mh .t").textContent(), "Feed video title");
  const thumbnailLookup = await page.evaluate(() => window.orbitMessages.find((m) => m.type === "video-lookup"));
  assert.equal(thumbnailLookup.url, "https://www.youtube.com/watch?v=TestVideo01");
  assert.equal(thumbnailLookup.at, null, "do not ask another player for the name of this card");
  await menu.locator(".extra-toggle").click();
  await page.getByRole("menuitem", { name: "All qualities & subtitles" }).click();
  const thumbnailDownload = await page.evaluate(() => window.orbitMessages.find((m) => m.type === "video"));
  assert.equal(thumbnailDownload.url, thumbnailLookup.url);
  assert.equal(thumbnailDownload.at, null);

  // YouTube uses these radio parameters on the ordinary JISOO music video card too.
  const musicUrl = "/watch?v=REUpBcmDbTE&list=RDREUpBcmDbTE&start_radio=1";
  await load(card(musicUrl).replace("yt-lockup-view-model-wiz__content-image", "ytLockupViewModelContentImage"));
  await page.mouse.move(180, 150);
  await pill.waitFor({ timeout: 1000 });
  // YouTube can update the preview's tracking parameters while the pointer rests on the card.
  await page.locator("#player a").first().evaluate((link) => { link.href = "/watch?v=REUpBcmDbTE&pp=tracking"; });
  await page.waitForTimeout(1750);
  assert.equal(await pill.isVisible(), true, "the icon stays available while the music card is hovered");
  await pill.locator(".go").hover();
  await arrow.click();
  assert.equal(await page.evaluate(() => window.orbitMessages.find((m) => m.type === "video-lookup")?.url),
    "https://www.youtube.com/watch?v=REUpBcmDbTE");
  await page.mouse.wheel(0, 100);
  await pill.waitFor({ state: "hidden" });
  await nextPaint();
  assert.equal(await pill.isVisible(), false, "wheel input dismisses the hovered card icon");
  await page.mouse.move(170, 150);
  await pill.waitFor();
  await page.mouse.move(400, 150);
  await pill.waitFor({ state: "hidden" });

  // YouTube can put a detached, transparent player above the card before it loads a video.
  // Its media link has a zero-height box. The enclosing preview and original card are visible.
  const floating = (href) => `<ytd-video-preview style="position:fixed;left:8px;top:90px;width:340px;height:180px;z-index:2">
    <div style="position:relative;width:100%;height:100%;overflow:hidden">
      <a class="ytdVideoPreviewMediaContainerLink" style="display:block;height:0" href="${href}">
        <span style="position:absolute;inset:0;opacity:0"></span>
      </a>
    </div>
  </ytd-video-preview>`;
  await load('<style>body {height:0;overflow-y:scroll}</style>' + card("/watch?v=TestVideo01") + floating("/watch?v=TestVideo01"));
  await page.mouse.move(180, 150);
  assert.ok(await page.evaluate(() => document.elementFromPoint(180, 150).closest("ytd-video-preview")));
  await pill.waitFor();
  await pill.locator(".go").hover();
  await arrow.click();
  assert.equal(await menu.locator(".mh .t").textContent(), "Feed video title");

  // If the original card does not match the floating preview, use the preview's own permalink.
  await load(card("/watch?v=TestVideo01") + floating("/watch?v=OtherVideo1"));
  await page.mouse.move(180, 150);
  await pill.waitFor();
  await pill.locator(".go").hover();
  await arrow.click();
  assert.equal(await page.evaluate(() => window.orbitMessages.find((m) => m.type === "video-lookup")?.url),
    "https://www.youtube.com/watch?v=OtherVideo1");

  for (const collection of [
    `<ytd-radio-renderer>${card(musicUrl)}</ytd-radio-renderer>`,
    card(musicUrl).replace(/yt-thumbnail-view-model/g, "yt-collection-thumbnail-view-model")
      .replace("yt-lockup-view-model-wiz__content-image", "ytLockupViewModelContentImage"),
    card("/playlist?list=PLTestPlaylist"),
  ]) {
    await load(collection);
    await page.mouse.move(180, 150);
    await nextPaint();
    assert.equal(await pill.isVisible(), false, "a collection card has no individual video action");
    await page.mouse.move(400, 150);
  }

  // A detached preview player can mount after hover has already settled.
  await load('<ytd-video-preview id="player" style="top:90px;width:340px;height:180px"></ytd-video-preview>');
  await page.mouse.move(180, 150);
  await nextPaint();
  assert.equal(await pill.isVisible(), false);
  await page.locator("#player").evaluate((player) => player.append(document.createElement("video")));
  await pill.waitFor();

  // A saved top-right position still leaves the YouTube close control free to click.
  const saved = { "www.youtube.com": { x: 1, y: 0 } };
  for (const fixture of [
    '<ytd-miniplayer id="player"><video></video><button id="native-close">Close</button></ytd-miniplayer>',
    '<div id="player" class="ytp-player-minimized"><video></video><button id="native-close">Close</button></div>',
  ]) {
    await load(fixture, saved, true);
    await page.mouse.move(180, 460);
    await pill.waitFor();
    const buttonBox = await pill.boundingBox();
    const closeBox = await close.boundingBox();
    assert.ok(buttonBox.y >= closeBox.y + closeBox.height, "toolbar sits below the miniplayer controls");
    await close.click();
  }

  // A small sound control can intersect the trigger between its center and corner hit tests.
  const preview = '<div id="player" class="html5-video-player"><video></video><button id="mute" aria-label="Mute" style="position:absolute;right:30px;top:22px;width:8px;height:8px;padding:0;border:0"></button></div>';
  await load(preview);
  await page.mouse.move(180, 460);
  await pill.waitFor();
  const shifted = await pill.boundingBox();
  assert.ok(shifted.x > 200, "stay close to the top-right corner when a native control is in the way");
  await page.locator("#mute").click();
  await page.mouse.move(180, 460);
  await page.locator("#mute").evaluate((mute) => { mute.style.opacity = "0"; });
  await page.waitForFunction(() => {
    const r = document.querySelector("video").getBoundingClientRect();
    const b = document.querySelector("odm-video-button").shadowRoot.querySelector(".pill").getBoundingClientRect();
    return r.right - b.right <= 12;
  });

  // A clipped player can extend beyond its card on both axes.
  await load('<div id="clip" style="position:absolute;top:30px;left:20px;width:280px;height:120px;overflow:hidden"><div id="player" style="position:relative;top:-45px;left:-24px"><video></video></div></div>');
  await page.mouse.move(150, 80);
  await pill.waitFor();
  const clippedPill = await pill.boundingBox();
  const clip = await page.locator("#clip").boundingBox();
  assert.ok(clippedPill.x >= clip.x && clippedPill.x + clippedPill.width <= clip.x + clip.width);
  assert.ok(clippedPill.y >= clip.y && clippedPill.y + clippedPill.height <= clip.y + clip.height);

  // A positioned video can remain visible through a collapsed intermediate wrapper.
  await load('<div id="player" style="top:90px;width:340px;height:180px"><div style="height:0;overflow:hidden"><video style="position:absolute;inset:0"></video></div></div>');
  await page.mouse.move(180, 150);
  assert.equal(await page.evaluate(() => document.elementFromPoint(180, 150).localName), "video");
  await pill.waitFor();

  // Four busy corners should send the trigger to a clear edge instead of hiding it.
  const cornerControls = ["top:0;left:0", "top:0;right:0", "bottom:0;left:0", "bottom:0;right:0"]
    .map((position) => `<button style="position:absolute;width:64px;height:48px;${position}">Control</button>`).join("");
  await load(`<div id="player" class="html5-video-player" style="top:90px"><video></video>${cornerControls}</div>`);
  await page.mouse.move(180, 150);
  await pill.waitFor();
  const crowded = await pill.boundingBox();
  assert.ok(crowded.x > 180 && crowded.y < 150, "stay close to the top-right with four busy corners");

  // Site-controlled scrolling in a sibling panel does not affect the video's position.
  await load('<div id="player" style="top:90px"><video></video></div><aside id="sidebar" style="position:absolute;right:0;width:80px;height:70px;overflow:auto"><div style="height:250px"></div></aside>');
  await page.mouse.move(180, 150);
  await pill.waitFor();
  await page.locator("#sidebar").evaluate((sidebar) => { sidebar.scrollTop = 40; });
  await nextPaint();
  assert.ok(await pill.isVisible(), "an unrelated scrollbar leaves the trigger visible");

  // Real wheel input on the trigger hides it without requiring the pointer to leave.
  await load('<div id="player" style="position:absolute;top:120px"><video></video></div><div style="height:2000px"></div>');
  await page.mouse.move(180, 180);
  await pill.locator(".go").hover();
  await page.mouse.wheel(0, 90);
  await pill.waitFor({ state: "hidden" });
  await page.waitForFunction(() => scrollY > 0);
  await nextPaint();
  assert.equal(await pill.isVisible(), false, "scrolling does not summon a different video under the same pointer");
  await page.mouse.move(150, 140);
  await pill.waitFor();

  // Scroll within a feed, including scrolling initiated by site code.
  await load('<div id="feed" style="width:380px;height:340px;overflow:auto"><div id="player" style="position:relative;top:35px"><video></video></div><div style="height:1000px"></div></div>');
  await page.mouse.move(180, 110);
  await pill.locator(".go").hover();
  await page.locator("#feed").evaluate((feed) => { feed.scrollTop = 60; });
  await pill.waitFor({ state: "hidden" });

  // Small preview shifts can keep a valid anchor while the pointer crosses the menu.
  await load('<div id="player"><video></video><a href="/watch?v=fixture1">Video</a></div>');
  await page.mouse.move(180, 460);
  await pill.waitFor();
  const idle = await pill.boundingBox();
  assert.equal(idle.width, 30, "the idle trigger shows only the Orbit mark");
  assert.equal(idle.height, 30, "keep the smaller trigger on compact previews");
  await pill.locator(".go").hover();
  await arrow.click();
  const opening = await menu.boundingBox();
  const anchor = await arrow.boundingBox();
  assert.ok(opening.x >= 8 && opening.x + opening.width <= 792, "menu starts within the viewport");
  assert.ok(opening.y + opening.height <= anchor.y, "menu opens above a toolbar near the bottom");

  await page.locator("#player").evaluate((el) => { el.style.left = "24px"; });
  await page.mouse.move(opening.x + 12, opening.y + 40, { steps: 10 });
  await nextPaint();
  assert.deepEqual(await menu.boundingBox(), opening, "movement over the menu cannot switch its horizontal alignment");
  assert.deepEqual(await arrow.boundingBox(), anchor, "the arrow remains stationary while the menu is open");

  // When the qualities arrive, the menu grows along the same anchor, with every edge still visible.
  await page.evaluate((choices) => window.finishLookup({ lookup: {
    state: "ready", token: "test-quality", default_choice: 3, title: "Orbit video fixture",
    choices,
  } }), choices);
  await menu.locator(".rows button").first().waitFor();
  const ready = await menu.boundingBox();
  assert.equal(ready.x, opening.x);
  assert.ok(ready.height > opening.height);
  assert.ok(ready.y >= 7 && Math.abs(ready.y + ready.height - opening.y - opening.height) <= 1);
  assert.deepEqual(await arrow.boundingBox(), anchor, "the trigger stays fixed after lookup");
  const rows = menu.locator(".rows button[data-choice]");
  assert.equal(await rows.count(), 6);
  assert.match(await menu.locator(".confirm").innerText(), /Download 720p\s+72 MB/);
  await page.keyboard.press("ArrowDown");
  assert.deepEqual(await page.evaluate(() => window.playerKeys), []);
  await page.keyboard.press("Enter");
  assert.equal(await page.evaluate(() => window.orbitMessages.filter((m) => m.type === "video-pick").length), 0);
  await menu.locator(".show-more").click();
  assert.equal(await rows.count(), 8);
  await menu.locator(".rows").hover();
  await page.mouse.wheel(0, 64);
  await nextPaint();
  assert.equal(await menu.isVisible(), true, "wheel inside the quality list leaves the picker open");
  await menu.locator('[data-kind="audio"]').click();
  assert.equal(await rows.count(), 2);
  assert.match(await rows.first().innerText(), /WEBM · 133 kbps.*Original/s);
  assert.match(await menu.locator(".confirm").innerText(), /Download audio/);
  await menu.locator(".extra-toggle").click();
  const extra = page.getByRole("menu");
  assert.equal(await extra.getByRole("menuitem").count(), 4);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await extra.waitFor({ state: "hidden" });
  const cookies = menu.getByRole("switch", { name: "Use site cookies" });
  await cookies.click();
  assert.equal(await cookies.isChecked(), false);
  assert.equal(await page.evaluate(() => window.orbitMessages.findLast((m) => m.type === "video-lookup")?.useSiteCookies), false);
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });

  // A moved player must never leave the trigger floating above another card.
  await load('<div id="player" style="top:100px"><video></video></div>');
  await page.mouse.move(170, 160);
  await pill.locator(".go").hover();
  await page.locator("#player").evaluate((el) => { el.style.left = "430px"; });
  await pill.waitFor({ state: "hidden" });
  await page.mouse.move(550, 160);
  await pill.waitFor();
  assert.ok((await pill.boundingBox()).x > 430);

  await load('<div id="player" style="top:32px;left:400px"><video style="background:#29213e"></video><a href="/watch?v=fixture2">Video</a></div>');
  await page.mouse.move(530, 100);
  await pill.locator(".go").hover();
  await arrow.click();
  await page.evaluate((choices) => window.finishLookup({ lookup: {
    state: "ready", token: "confirm-quality", default_choice: 0, title: "Orbit video fixture",
    choices,
  } }), choices);
  await rows.first().waitFor();
  await rows.nth(3).click();
  assert.match(await menu.locator(".confirm").innerText(), /Download 720p/);
  const assets = await page.evaluate(() => {
    const shadow = document.querySelector("odm-video-button").shadowRoot;
    return {
      rendered: [...shadow.querySelectorAll(".menu svg")].every((svg) => svg.getBBox().width > 0),
      external: shadow.querySelectorAll("img, image, use[href]:not([href^='#'])").length,
      blocked: window.blockedAssets,
    };
  });
  assert.deepEqual(assets, { rendered: true, external: 0, blocked: [] });
  const previews = new URL("../../target/extension-previews/", import.meta.url);
  mkdirSync(previews, { recursive: true });
  await menu.screenshot({ path: fileURLToPath(new URL("video-quality-picker.png", previews)) });
  await menu.locator(".confirm").click();
  assert.deepEqual(await page.evaluate(() => window.orbitMessages.filter((m) => m.type === "video-pick")), [
    { type: "video-pick", token: "confirm-quality", choice: 3 },
  ]);
  assert.match(await pill.innerText(), /Sending to Orbit/);
  await page.evaluate(() => window.finishPick({ lookup: { state: "started" } }));
  await pill.getByText("Added to Orbit").waitFor();
  console.log("Video overlay browser checks passed: YouTube thumbnails and delayed previews, scrolling, positioning, strict CSP, and quality picker.");
} finally {
  await browser.close();
}
