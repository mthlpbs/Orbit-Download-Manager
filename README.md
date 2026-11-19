# Orbit Download Manager

<p align="center"><img src="docs/images/logo.png" alt="Orbit Download Manager logo" width="160"></p>

Orbit Download Manager is a free, open source download manager for **Windows 10 and 11**.
Maintained by **Mithila Prabashwara**.

**Derived from [Mister Download Manager](https://github.com/anefzaoui/misterdownloadmanager),
created by Ahmed Nefzaoui.** Orbit continues that project as a Windows-only fork under the
GNU GPL, version 3 or later. Original contributors and third-party credits are retained in
[ATTRIBUTION.md](ATTRIBUTION.md).

[Download the latest release](https://github.com/mthlpbs/Orbit-Download-Manager/releases/latest) ·
[User guide](docs/README.md) ·
[Report a problem](https://github.com/mthlpbs/Orbit-Download-Manager/issues)

## Features

- Parallel downloads, pause/resume, queues, scheduling and speed limits.
- Browser integration for Chrome, Edge, Brave, Opera, Vivaldi and Firefox.
- Video and gallery downloads using verified yt-dlp, ffmpeg and gallery-dl helpers.
- Public MEGA file and folder downloads, with decryption, integrity checks and proxy support.
- Torrents and magnet links, RSS feeds and checksum verification.
- Themes, an English interface, portable mode, command line and MCP support.

## Download for Windows

Windows x64, ARM64 and 32-bit x86 builds are supported. Release files use the prefix
`OrbitDownloadManager-<version>-`:

| Processor | Installer | Portable archive | Plain archive |
| --- | --- | --- | --- |
| x64 | `setup.exe` | `portable.zip` | `x64.zip` |
| ARM64 | `setup-arm64.exe` | `portable-arm64.zip` | `arm64.zip` |
| x86 (32-bit Windows) | `setup-x86.exe` | `portable-x86.zip` | `x86.zip` |

The installer registers browser integration and optional torrent associations. The portable
archive keeps settings beside the application in its `data` folder. Orbit uses its own
installation identity, executables and data directory, separate from Mister Download Manager.

Browser extensions are included in the installer and published as Chromium ZIP and Firefox XPI
release assets. Load the Chromium extension unpacked from its extracted folder. Firefox's
unsigned XPI can be loaded temporarily through `about:debugging`; permanent installation in
standard Firefox requires a separately signed Orbit add-on. Orbit has no browser store listing
yet. See [browser integration](docs/browser-integration.md).

## MEGA downloads

Use **Tasks → Add new download** and paste the complete public link, including the
`#` decryption key. Public folders open in Link grabber so you can select their files.
Classic `#!` and `#F!` links are supported too. Account-only and password-protected links
require a public share link first.

MEGA API and file requests use **Preferences → Settings → Proxy / Socks**, including authenticated HTTP
and SOCKS proxies. Pause/resume keeps decrypted progress, and Orbit checks MEGA’s CBC-MAC
before completing each file. A transfer-quota error keeps the partial file for later resume.
The protocol implementation is adapted from MegaBasterd; see [credits](ATTRIBUTION.md).

## Preferences and video helpers

**Preferences** contains **yt-dlp**, **Torrent**, and **Settings** (last).
The **yt-dlp** settings detect existing yt-dlp and ffmpeg installs, accept custom executable
paths, and keep Orbit's verified helper-download option for programs that are missing.

The video download window starts with quality presets; **Advanced** reveals individual
formats and their IDs. Set a default quality, container and preferred source codec in
**Preferences → yt-dlp → Download defaults**. MKV is the default container. Source codec
selection does not re-encode video. Subtitle languages, automatic captions, SRT embedding,
thumbnails, chapters, metadata and info JSON attachments are configurable. Missing thumbnails
or subtitles are skipped. Thumbnails and info JSON attachments work with MKV; container
support varies for other embedded content.

The default filename template is `%(title)s.%(ext)s`; `id`, `uploader` and `upload_date`
are also supported. Use a filename without directory components. Deno is detected
automatically, or select its executable under **Programs and configuration**. YouTube's
`ejs:github` remote components are enabled by default and can be disabled.

Select **Use yt-dlp configuration file** before looking up a video to use a discovered
user-folder `yt-dlp.conf` or a custom file selected in settings. The button changes to
**Download** and manual controls are disabled; clear the checkbox to restore them.
This choice stays visible after lookup and can be saved as the default. Settings also
provide an editor for the selected config. Merely discovering a config does not enable it.
Format, sort, audio, subtitle, cookie and metadata preferences are supported.
Orbit manages transfer and output paths; external downloaders, executable
hooks, nested configs, and mode/progress overrides are ignored. Streams unsupported by
Orbit's engine fail with an explanation instead of launching another downloader.
Lookup metadata is reused when starting the download to avoid a second page lookup.

Category folders are available again under **Settings → Save to**. The Categories sidebar
starts hidden and can be shown using **View → Show categories**. **Add new download** and
**Add batch download from clipboard** extract links from surrounding prose; MEGA link
fragments and keys are retained. MEGA partial data and resume metadata are kept in Orbit's
`cache/downloads` folder. Only a verified, completed file is moved to its category folder.

## Build from source

Use Windows, Rust stable 1.98 or later with the MSVC toolchain, CMake, LLVM and NASM.
Install Visual Studio Build Tools with the C++ desktop workload and Windows SDK.
Install Inno Setup 6.3 or later to produce the installer, and Node.js for extension checks.

```powershell
git clone https://github.com/mthlpbs/Orbit-Download-Manager.git
cd Orbit-Download-Manager
cargo build --release -p mdm-app -p mdm-nmhost -p mdm-cli -p mdm-mcp
cargo run -p mdm-app
powershell -ExecutionPolicy Bypass -File tools/release/build-windows.ps1 -Date 2026-11-19 -Arch x64 -InstallerOnly
```

Replace `x64` with `arm64` or `x86` for another Windows processor. Internal Rust crate names
retain the `mdm-` prefix; the installed executables use `orbitdownloadmanager`.

The CI workflow formats, lints and tests Windows builds. The Release workflow publishes
Windows installers, archives and browser extensions on a matching date-version tag such as
`v26.11.19`; a manual run
keeps build artifacts without publishing a release.

## Versioning

Orbit uses the release date: **26.11.19** means November 19, 2026. Month and day use
numeric components without leading zeroes to satisfy Cargo and browser-store version rules.
Before building, run `node tools/release/set-version.mjs 2026-11-19` (or omit the date to use
today). This updates the workspace, lockfile, installer and both extension manifests together.

## Release updates

Update checks, downloads, crash reports and source/help links target
[mthlpbs/Orbit-Download-Manager](https://github.com/mthlpbs/Orbit-Download-Manager).
Automatic installation still requires a valid signature and checksum. Configure these on the
Orbit repository before publishing signed updates:

- Actions variable `ORBIT_RELEASE_PUBLIC_KEY`: an ASCII-armored OpenPGP public key, compiled
  into the application. Use a legacy Ed25519 (OpenPGP algorithm 22) or RSA signing key.
- Actions secret `ORBIT_RELEASE_KEY`: the matching ASCII-armored private signing key usable
  noninteractively by GnuPG. Never commit it.

Local builds can set `ORBIT_RELEASE_PUBLIC_KEY` in the build environment. Without a configured
key, releases remain downloadable and update checks work, but automatic installation refuses
unsigned/untrusted releases. The upstream project's private key is not used by Orbit.

## Logo

The supplied logo is saved as [art/Orbit Download Manager.svg](art/Orbit%20Download%20Manager.svg).
It supplies the Windows executable/installer icon, app and tray icons, browser extension icons,
and README logo. Regenerate the assets with:

```powershell
npm ci --prefix tools/icon
node tools/icon/build_icon.mjs
```

## License and credits

[GNU GPL-3.0-or-later](LICENSE). Derived from Mister Download Manager by Ahmed Nefzaoui and
its contributors. Orbit author/maintainer: Mithila Prabashwara. See [credits](ATTRIBUTION.md)
and [contributing](CONTRIBUTING.md).
