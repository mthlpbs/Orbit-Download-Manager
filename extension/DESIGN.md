# Orbit browser extension

Implemented from [Orbit Download Manager — browser redesign](https://www.figma.com/design/E26c9gs2vyMdV58zwQChac/Orbit-Download-Manager-Logo?node-id=7-145), including “What changed” notes and the popup/Settings frames in both themes.

The populated popup is 360 × 526 px; Settings is 360 × 598 px (including the global Show download button switch). Lists scroll inside the popup. Connection and rule notices reduce the list’s available height so browser controls remain reachable. Preferences save immediately; returning from Settings retains the list position. API-provided thumbnails stay dynamic, with the original Figma fallback images when no web poster is available.

The toolbar popup uses bundled Figma icons, thumbnails and Inter (`fonts/OFL.txt`). The on-video picker uses the system font and inline SVGs generated from the bundled assets by `node tools/icon/build_overlay_icons.mjs`.

The [on-video download panel](https://www.figma.com/design/E26c9gs2vyMdV58zwQChac/Orbit-Download-Manager-Logo?node-id=82-1052) uses the supplied Orbit glyph and the glass picker. Hovering over the trigger reveals the quick download and the quality picker. Inside the picker, Video and Audio have separate lists. Selecting a quality updates the confirmation button, which shows the size when available. Longer lists initially show six choices. The footer contains the site cookies switch and the menu for subtitles, visibility and button settings.

Run behavior tests with `node tools/extension/test.mjs`. With Playwright and Chromium installed, run `node tools/extension/test-popup.mjs` and `node tools/extension/test-video-overlay.mjs` for browser and layout checks. `PLAYWRIGHT_MODULE` and `CHROME_PATH` can point to an existing local runtime. Preview images are written under `target/extension-previews/`.
