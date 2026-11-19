# Orbit browser extension

Implemented from [Orbit Download Manager — browser redesign](https://www.figma.com/design/E26c9gs2vyMdV58zwQChac/Orbit-Download-Manager-Logo?node-id=7-145), including “What changed” notes and the popup/Settings frames in both themes.

The populated popup is 360 × 526 px; Settings is 360 × 598 px (including the global Show download button switch). Lists scroll inside the popup. Connection and rule notices reduce the list’s available height so browser controls remain reachable. Preferences save immediately; returning from Settings retains the list position. API-provided thumbnails stay dynamic, with the original Figma fallback images when no web poster is available.

Icons in assets/ are original transparent Figma exports, used as CSS masks to follow the theme. The fallback images are original Figma exports. Fonts are bundled locally; fonts/OFL.txt contains Inter’s license. No temporary Figma asset URLs are used at runtime.

Run behavior tests with node tools/extension/test.mjs. For browser and layout checks, install Playwright with Chromium and run node tools/extension/test-popup.mjs. PLAYWRIGHT_MODULE and CHROME_PATH can point to an existing local runtime. Preview images are written under target/extension-previews/.
