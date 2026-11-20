# Getting started

[← Back to docs index](README.md)

Orbit Download Manager supports Windows 10 and Windows 11 (x64, ARM64 and 32-bit).

## Choose a download

Every version is on the [Releases](https://github.com/mthlpbs/Orbit-Download-Manager/releases) page. For Windows there are three files:

| File | What it is |
|---|---|
| `OrbitDownloadManager-<version>-setup.exe` | The installer. Pick this one unless you have a reason not to. It can set up browser integration and open `.torrent` files and magnet links with ODM. |
| `OrbitDownloadManager-<version>-portable.zip` | A portable copy. The download list and settings stay in a `data` folder next to the program, and nothing is written to the registry. Browser integration needs the installer. See [Portable mode](portable.md). |
| `OrbitDownloadManager-<version>-x64.zip` | The same programs without an installer. It keeps its data in `%APPDATA%\orbitdownloadmanager`, like an installed copy. |

These three are for 64-bit Windows on an Intel or AMD processor (x64), the usual PC. From the
Orbit releases, each comes for two more kinds of PC too, with the processor in its name:

| Files | For |
|---|---|
| `...-setup-arm64.exe`, `...-portable-arm64.zip`, `...-arm64.zip` | **Windows on ARM** (PCs with a Snapdragon or another ARM processor). On Windows 11 the x64 files run there too, translated by Windows, more slowly. The ARM64 setup installed over the x64 one replaces it; your list and settings stay, as they are kept in your profile, not with the program. |
| `...-setup-x86.exe`, `...-portable-x86.zip`, `...-x86.zip` | **32-bit Windows 10.** Not sure which you have? **Settings → System → About → System type** says it. ODM can't download ffmpeg for you there (none is built for 32-bit Windows that ODM can check): to get the best video qualities, choose an ffmpeg of your own in **Helpers...** ([Video](video.md#yt-dlp-and-ffmpeg)). |

**Help → Check for updates** updates each copy to the files for its own processor.


## Install with the setup

1. Run `OrbitDownloadManager-<version>-setup.exe`.
2. The installer and application use English.
3. Choose whether to install for you only or for everyone on the computer. For you only needs no administrator rights and goes to `%LOCALAPPDATA%\Programs\Orbit Download Manager`. For everyone goes to `Program Files`.
4. Pick what the setup should do:
   - **Set up browser integration (Chrome, Edge, Brave, Firefox)**
   - **Open .torrent files and magnet links with Orbit Download Manager**
   - **Start Orbit Download Manager when Windows starts**
   - **Create a desktop shortcut** (off unless you tick it)
5. Click **Install**, then **Finish** to start ODM.

If ODM is already running, the setup asks it to save and close first. To update, use **Help → Check for updates → Update now**, or run the setup of the new version (or update with [WinGet](#with-winget)); your list and settings stay.

Browser integration needs one more step in each browser: adding the extension. In Firefox, click **Add to Firefox** on [its page on addons.mozilla.org](https://addons.mozilla.org/firefox/addon/mister-download-manager/); in Chrome, Edge, Brave, Opera and Vivaldi, **Add to Chrome** on [its page in the Chrome Web Store](https://chromewebstore.google.com/detail/mister-download-manager/ejoleppdfgjdndddcjoehopknmlglojp). [Browser integration](browser-integration.md) shows how.

### Uninstall

Uninstall ODM from Windows **Settings → Apps**. The uninstaller asks whether to delete your download list and settings too. Your downloaded files are never touched.

## Use a zip copy

1. Unzip the file to a folder you can write to (not straight into `Program Files`).
2. Run `orbitdownloadmanager.exe`.

The zips hold the same programs as the setup: the app, `orbitdownloadmanager-cli.exe` (downloads from the command line; see [Command line and automation](command-line.md)) and the `extension` folder.

## First start

Orbit opens in English and offers to install its video helpers.

## The video helpers

Next, ODM offers its two free video helpers: **yt-dlp**, which finds the video on a page, and
**ffmpeg**, which joins its picture and sound. Click **Yes, download them (recommended)** and they
download in the background, about 100 MB, once. **Not now** leaves them for later: the video
window offers them when you first need them. More in [Videos](video.md).

![ODM offers yt-dlp and ffmpeg on its first start, with Yes, download them (recommended) and Not now](images/help/first-helpers.webp)
*Yes downloads both helpers in the background, once.*

## The main window

![The main window: the menu bar, the toolbar, the categories on the left and the download list on the right](images/help/main-window.webp)
*1 the menu bar, 2 the toolbar, 3 the categories, 4 the download list.*

From top to bottom:

- **Menu bar.**
  - **Tasks** — add downloads, torrents, magnet links, batch downloads and videos from a page; **Torrent settings**; **Link grabber**; **RSS feeds**; show the drop target; import and export the list; exit.
  - **File** — the selected download: **Stop Download**, **Remove**, **Download Now**, **Redownload**.
  - **Downloads** — **Pause All**, **Stop All**, **Delete All Completed**, **Find**, the **Scheduler**, queues, the **Speed Limiter** and **Options**.
  - **View** — show or hide the categories, sort the list (**Arrange files**), **Dark Mode support**, **Always dark**, **Theme**, **Interface size** (every window larger, 100 % to 200 %; **Ctrl +**, **Ctrl −** and **Ctrl 0** too), **Text** (Windows: smooth or classic letters), **Draw with** (OpenGL or Direct3D on Windows).
  - **Help** — help pages and tutorials in the repository, **Keyboard shortcuts**, **Report a problem**, **Check for updates**, **About**, **Tell a friend** and **Support ODM** (its creator's GitHub Sponsors page: optional, nothing in ODM changes).
- **Toolbar.** **Add URL**, **Add Torrent**, **Resume**, **Stop**, **Stop All**, **Delete**, **Delete Completed**, **Options**, **Scheduler**, **Start Queue**, **Stop Queue**. The small arrow next to **Add Torrent**, **Start Queue** and **Stop Queue** opens more choices.
- **Categories** (left). Choose what the list shows: **All Downloads**, **Unfinished**, **Finished** and **Queues**, each with its categories. Drag the gap between the pane and the list to make it wider.
- **Download list** (right). One row per download. See [The download list](download-list.md).

## Dark mode

ODM follows your system: when Windows (or your desktop's light/dark preference on Linux and macOS) is set to dark mode, so is ODM. To keep ODM light whatever the system does, turn off **View → Dark Mode support**. To keep it dark whatever the system does, for example on a desktop that reports no preference, turn on **View → Always dark**.

![The View menu open, with Dark Mode support and Always dark](images/help/view-dark-mode.webp)
*1 Dark Mode support follows the system; 2 Always dark keeps ODM dark.*

## Themes

**View → Theme** changes ODM's look at once, keeping its layout:

- **Classic**: ODM's own look, the default.
- **Native**: your system's colours. Windows' accent colour (Settings → Personalization →
  Colors) and, when it is on, its contrast theme; the desktop's accent colour and its higher
  contrast setting on Linux (GNOME 47 and later, KDE Plasma); the accent colour on macOS. It
  follows them when you change them.
- **High contrast**: black and white, a bright accent, every edge drawn, text at 7:1 or more.
- **Metro**: Windows 8's flat look: square corners, flat buttons and boxes, a coloured title bar
  with the title centred (on Windows, where ODM draws it), and one-colour icons (Microsoft's
  Fluent UI System Icons).
- **Adwaita**: GNOME's look: round corners, flat grey buttons, a rounded focus ring, GNOME's
  check boxes, a header bar with the title in bold and round buttons (on Windows, where ODM
  draws it), and GNOME's Adwaita icons, in colour for files and categories, one colour in the
  toolbar.
- **Breeze**: KDE Plasma's look: rounded corners, buttons and boxes that take the accent's edge
  under the pointer or when focused, KDE's check boxes and focus frame, KDE's title bar with the
  title centred and round buttons (on Windows, where ODM draws it), and KDE's Breeze icons.
- **Luna**: Windows XP's look: buttons shaded from white with an orange glow under the pointer,
  green ticks, square text boxes, progress in green blocks, a deep blue title bar with the title
  in bold (on Windows, where ODM draws it), and colourful icons drawn for ODM in that manner.
- **Aero**: Windows 7's look: glossy buttons that turn light blue under the pointer, quiet check
  boxes, a glossy green progress bar, a glassy title bar with the title on a soft glow (on
  Windows), and glossy icons drawn for ODM in that manner.
- **Aqua**: Mac OS X's look: gel buttons round at the ends (the default one blue), glossy check
  boxes, a blue focus ring, a blue gel progress bar, a grey title bar with small round buttons
  (on Windows), and icons drawn for ODM in that manner.
- **Emerald**: a white canvas with one near-black ink and a ladder of
  neutral greys for structure, emerald fills that always carry near-black text rather than white,
  6-point corners on every control, and Metro's one-colour icons.

![View → Theme open: Classic, Native, High contrast, Metro, Adwaita, Breeze, Luna, Aero, Aqua and Emerald, and Open themes folder](images/help/view-theme.webp)
*View → Theme switches at once, keeping the layout.*

<!-- pictures -->

- ![The main window's toolbar and categories in the Classic theme](images/help/theme-classic.webp)
  *Classic*
- ![The main window's toolbar and categories in the High contrast theme](images/help/theme-high-contrast.webp)
  *High contrast*
- ![The main window's toolbar and categories in the Adwaita theme](images/help/theme-adwaita.webp)
  *Adwaita*
- ![The main window's toolbar and categories in the Breeze theme](images/help/theme-breeze.webp)
  *Breeze*

Each has a light and a dark side: **Dark Mode support** and **Always dark** choose between
them as before. Theme files you put in the themes folder (**View → Theme → Open themes folder**)
are listed there too, and so are Base16 and pywal colour schemes (**View → Theme → Add a theme
file…**; **pywal** follows `wal` as it runs): [Themes](themes.md) says where that folder is, what
a theme file holds (`inherits`, colours, sizes, control shapes, icons), how a scheme's colours are
used, and how to make and share one.

## The tray icon

Closing the main window doesn't quit ODM. It hides in the notification area, and downloads, queues and schedules carry on. To make closing quit (a portable copy's folder can then be deleted), untick **Options → General → Keep running when closing the window**. If there is no icon for ODM at all — not in the notification area, nor behind the ^ hidden icons — the window minimizes to the taskbar instead, so downloads continue and there is a way back. An icon behind ^ can be clicked to bring the window back, and starting ODM again always does. On macOS the window closes into the Dock, as Mac apps' windows do: click ODM's icon in the Dock (or in the menu bar) to bring it back, and quit with **Cmd+Q** or **Orbit Download Manager → Quit**.

On Linux this needs a tray, which most desktops have (KDE Plasma, Xfce, Cinnamon, Ubuntu's GNOME or GNOME with the AppIndicator extension, waybar's tray). On a desktop without one (some tiling window managers), closing the window quits ODM, since there would be nowhere to bring it back from.

![ODM's icon in the top bar of Ubuntu's GNOME, its menu open: Open Orbit Download Manager, Add new download, Add batch download, Downloads in progress, Speed Limiter, Scheduler, the main queue, Options and Exit](images/tray-linux.webp)
*The tray icon's menu on Ubuntu's GNOME.*

- Click the tray icon to bring the window back.
- Right-click it for **Add new download**, **Add batch download**, **Downloads in progress**, **Speed Limiter**, **Scheduler**, the main download queue, **Options** and **Exit**.
- **Downloads in progress** lists the running downloads. Choose one to see its progress window.

Minimize hides the main window there too, while **Minimize to the notification area** is on in **Options → General**. It is on by default; where the tray has no icon, Minimize sends the window to the taskbar instead.

A download's progress window can go there as well, with its Minimize or with its **Hide to the notification area** button: a short dash with an arrow down to the corner.

On Windows that button is in the window's title bar, left of Minimize, and in the title bar's menu (Alt+Space). On Linux and macOS it's at the end of the tab row.

To bring the window back, choose it in **Downloads in progress**, or double-click the download in the list.

To quit, choose **Tasks → Exit** or **Exit** in the tray menu.

## Where ODM keeps its data

The download list and settings are in `%APPDATA%\orbitdownloadmanager` (`downloads.json`); on Linux in `~/.local/share/orbitdownloadmanager`, on a Mac in `~/Library/Application Support/orbitdownloadmanager`. A portable copy keeps them in its `data` folder instead. Downloaded files go to your **Downloads** folder, sorted into a folder per category; you can change that in **Options → Save to**.

## Next steps

- [Download your first file →](guide-first-download.md)
- [Send browser downloads to ODM →](browser-integration.md)
- [Learn the download list →](download-list.md)
