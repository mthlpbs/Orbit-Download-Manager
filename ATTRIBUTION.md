# Attribution

Orbit Download Manager is maintained by Mithila Prabashwara and is derived from
[Mister Download Manager](https://github.com/anefzaoui/misterdownloadmanager), created by
Ahmed Nefzaoui and its contributors. Upstream code remains under GPL-3.0-or-later.
Original copyright notices and third-party licenses remain applicable.

The Orbit logo was supplied by Mithila Prabashwara as `Dark.svg` and is stored as
`art/Orbit Download Manager.svg`. All app, installer, tray, extension and README logo sizes
are generated from it by `tools/icon/build_icon.mjs`.

# Third-party assets

| Asset | Author | Licence | Where |
|---|---|---|---|
| Lucide icons v1.63.0 (toolbar, dialog glyphs) | Lucide Icons and Contributors | ISC — `crates/app/assets/icons/lucide/LICENSE` | `crates/app/assets/icons/lucide/` |
| Tango Icon Theme 0.8.90, 25 of its icons (the icon before a file's or folder's name where the system has none of its own, and until the system's is ready: every window that names a file; this computer and the local network where a download's server is) | The Tango Desktop Project (Ulisse Perusin, Steven Garrity, Lapo Calamandrei, Ryan Collier, Rodney Dawes, Andreas Nilsson, Tuomas Kuosmanen, Garrett LeSage, Jakub Steiner), <http://tango.freedesktop.org> | Public domain — crates/app/assets/icons/tango/README.txt | crates/app/assets/icons/tango/ |
| flag-icons 7.6.0, all 250 country flags in one picture (where a download's server is: Download File Info, the progress window, Properties), drawn by `crates/geo/examples/flag_atlas.rs` | Panayiotis Lipiridis and contributors, <https://github.com/lipis/flag-icons> | MIT — `crates/geo/data/LICENSE-flag-icons` | `crates/geo/data/flags.png` |
| DB-IP IP to Country Lite, a monthly edition (which country a download's server is in, looked up on the computer, never sent anywhere), packed by `tools/geo/build.py` | DB-IP, <https://db-ip.com>: IP Geolocation by DB-IP | Creative Commons Attribution 4.0 International, <https://creativecommons.org/licenses/by/4.0/>; the edition used is in `crates/geo/data/SOURCE.txt` | `crates/geo/data/ipcountry.bin.xz` |
| *Big Buck Bunny* (the film in the README's pictures of the download button over a video; not in the app) | Blender Foundation, <https://peach.blender.org> | Creative Commons Attribution 3.0 | `docs/images/video-button-*.png` |
| Silk icon set 1.3 (Categories tree) | Mark James, <http://www.famfamfam.com/lab/icons/silk/> | Creative Commons Attribution 2.5 — `crates/app/assets/icons/silk/readme.txt` | `crates/app/assets/icons/silk/` |
| Fluent UI System Icons 1.1.345, 49 of its icons (the Metro theme's toolbar, categories, file types and title-bar buttons), recoloured white to be tinted | Microsoft Corporation, <https://github.com/microsoft/fluentui-system-icons> (commit 9eb184e) | MIT — `crates/app/assets/themes/metro/icons/LICENSE` | `crates/app/assets/themes/metro/icons/` (its magnet, Torrents, torrent-file and Hide to the notification area icons come from the upstream project) |
| adwaita-icon-theme 51.0, 50 of its icons (the Adwaita theme's toolbar, categories, file types and title-bar buttons), unchanged; its one-colour icons are drawn in the theme's text colour when read | The GNOME Project, <https://gitlab.gnome.org/GNOME/adwaita-icon-theme> (commit 70ecd52) | LGPL-3.0 (of LGPL-3.0 or CC BY-SA 3.0 US) — `crates/app/assets/themes/adwaita/icons/LICENSE`, `COPYING` | `crates/app/assets/themes/adwaita/icons/` (its PDF, torrent-file and Hide to the notification area icons come from the upstream project) |
| breeze-icons 6.31, 51 of its icons (the Breeze theme's toolbar, categories, file types and title-bar buttons), unchanged; its one-colour icons are drawn in the theme's text colour when read | Uri Herrera and the KDE Visual Design Group, <https://invent.kde.org/frameworks/breeze-icons> (commit 60ca517) | LGPL-3.0-or-later — `crates/app/assets/themes/breeze/icons/LICENSE` | `crates/app/assets/themes/breeze/icons/` (its PDF and Hide to the notification area icons come from the upstream project) |

The original Mister Download Manager icon was created by Ahmed Nefzaoui with ChatGPT.
Orbit replaces it with the supplied Orbit logo described above.

The Luna, Aero and Aqua themes' icons (`crates/app/assets/themes/luna/icons/`, `aero/icons/`,
`aqua/icons/`) were drawn for Mister Download Manager, as were the Metro, Adwaita and Breeze
icons identified as upstream artwork above; each set's `SOURCES.txt` lists them.

The README's block icons (`docs/images/icons/`: arrow, globe, gift, magnet) were made for it
with OpenAI’s GPT-5 Image Mini for the upstream project.

## Archive extraction (odm-extract)

The libraries Link grabber's archive extraction is built on (all compatible with ODM's GPL-3.0):

| Library | Licence | For |
|---|---|---|
| [zip](https://crates.io/crates/zip) 9.1 | MIT | .zip, with [deflate64](https://crates.io/crates/deflate64) (MIT) and WinZip AES ([RustCrypto](https://github.com/RustCrypto) aes, hmac, pbkdf2: MIT OR Apache-2.0) |
| [lzma-rust2](https://crates.io/crates/lzma-rust2) 0.23 | Apache-2.0 | .xz |
| [tar](https://crates.io/crates/tar) 0.4 | MIT OR Apache-2.0 | .tar |

7z and RAR archives are extracted by [7-Zip](https://www.7-zip.org/) (GNU LGPL, with the
unRAR restriction for its RAR code), run as a separate program when the user has it installed.
It is neither bundled nor linked: UnRAR's licence doesn't allow its code in a GPL program, and a
built-in 7z decoder made ODM about 1 MB larger.

System resources used at run time and **not** bundled: Windows fonts (Segoe UI, Tahoma,
Arial) and each system's own file-type icons (the Windows shell's, macOS Finder's, the Linux
desktop's icon theme).

ODM contains no artwork, text or code from Internet Download Manager (see the README).

## MEGA public-link downloads

Orbit’s native MEGA API, link-key derivation, attribute decryption and chunk CBC-MAC
verification are adapted from [MegaBasterd](https://github.com/tonikelope/megabasterd),
by tonikelope and contributors, under GPL-3.0. Reference revision:
`c4b8b76dcf03e726aac77f8959ab90f920765124` (`MegaAPI.java`, `CryptTools.java`,
`Download.java`). Orbit uses RustCrypto AES (MIT OR Apache-2.0), with its own engine’s
proxy, destination, progress and resume handling. MegaBasterd/Java is not bundled.
