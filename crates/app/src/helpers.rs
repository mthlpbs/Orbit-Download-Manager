//! The first start's offer of the video helpers (user request 2026-11-01): yt-dlp finds a page's
//! video, ffmpeg joins its picture and sound. Both are asked for once, explained, and downloaded
//! on Yes (through MDM's own engine, checked against their publishers' signed checksums: see
//! mdm_tools); Not now leaves them to Tasks → Download video from a page, which offers them again.

use std::sync::Arc;
use std::sync::Mutex;

use egui::{Pos2, Ui};

use crate::dialogs::{Outcome, window};
use crate::icons::Icon;
use crate::winui::{self, ButtonKind, r};

/// The helpers' download, one after the other (user decision 2026-11-01: one at a time, one
/// progress bar for both).
#[derive(Default)]
struct State {
    /// The helper downloading now: its name, its place (1-based) and how many there are.
    current: Option<(&'static str, usize, usize)>,
    /// The helpers done: name and version.
    ready: Vec<(&'static str, String)>,
    /// The helper that failed, why, and what to do about it (#816; the ones after it are not
    /// tried).
    error: Option<(&'static str, String, Option<String>)>,
    done: bool,
}

struct Job {
    /// The current helper's bytes.
    progress: Arc<mdm_tools::update::Progress>,
    state: Arc<Mutex<State>>,
}

pub struct Helpers {
    dir: Option<std::path::PathBuf>,
    /// Once Yes was pressed.
    job: Option<Job>,
}

/// Whether to offer the helpers at this start: never asked, and one of them is missing (ffmpeg
/// counts only where MDM can install one, Windows; elsewhere the system's is used).
pub fn worth_asking() -> bool {
    let Some(dir) = mdm_tools::tools_dir() else {
        return false;
    };
    mdm_tools::installed(&dir).is_none()
        || (cfg!(windows) && mdm_tools::ffmpeg::installed(&dir).is_none())
}

impl Helpers {
    pub fn new() -> Self {
        Helpers {
            dir: mdm_tools::tools_dir(),
            job: None,
        }
    }

    /// Download the missing helpers one after the other, in the background: they go on when
    /// the window is closed. Try again starts over with the ones still missing.
    fn start(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        let mut list = Vec::new();
        if mdm_tools::installed(&dir).is_none() {
            list.push("yt-dlp");
        }
        // ffmpeg: MDM's own on Windows (it has no build elsewhere, where the system's is used).
        if cfg!(windows) && mdm_tools::ffmpeg::installed(&dir).is_none() {
            list.push("ffmpeg");
        }
        let job = Job {
            progress: Arc::default(),
            state: Arc::default(),
        };
        let (progress, state, ctx) = (job.progress.clone(), job.state.clone(), ctx.clone());
        let net = crate::settings::own_net();
        rt.spawn(async move {
            let total = list.len();
            for (k, name) in list.into_iter().enumerate() {
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    progress.got.store(0, Relaxed);
                    progress.total.store(0, Relaxed);
                }
                state.lock().unwrap().current = Some((name, k + 1, total));
                let res = if name == "ffmpeg" {
                    mdm_tools::ffmpeg::install_latest(&dir, &net, Some(&progress)).await
                } else {
                    mdm_tools::install_latest(&dir, &net, Some(&progress)).await
                };
                let mut s = state.lock().unwrap();
                match res {
                    Ok(installed) => s.ready.push((name, installed.version)),
                    Err(e) => {
                        tracing::warn!("helper download failed: {name}: {e}");
                        s.error = Some((
                            name,
                            crate::errors::tools(&e),
                            crate::errors::tools_advice(&e, name),
                        ));
                        break;
                    }
                }
            }
            let mut s = state.lock().unwrap();
            s.current = None;
            s.done = true;
            drop(s);
            ctx.request_repaint();
        });
        self.job = Some(job);
    }

    /// Open while shown (after Yes too, with the downloads' progress); Closed on Not now, Close
    /// or the window's X.
    pub fn show(&mut self, ctx: &egui::Context, rt: &tokio::runtime::Handle) -> Outcome<()> {
        let (running, failed) = match &self.job {
            Some(job) => {
                let s = job.state.lock().unwrap();
                (!s.done, s.error.is_some())
            }
            None => (false, false),
        };
        if running {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        let mut answer = None;
        // A long translation's subline makes the banner taller (winui::banner).
        let extra = crate::dialogs::grow_extra(ctx, "helpers-banner")
            + crate::dialogs::grow_extra(ctx, "helpers-error")
            + crate::dialogs::grow_extra(ctx, "helpers-advice");
        window(
            ctx,
            "helpers",
            // i18n: title of the first start's window that offers yt-dlp and ffmpeg
            "Get ready for video downloads",
            // Taller once Yes is pressed: the progress under the last section.
            [
                520.0,
                if self.job.is_some() { 418.0 } else { 382.0 } + extra,
            ],
            false,
            winui::BG(),
            |ui, o| {
                self.body(ui, o, running, failed, &mut answer);
                match answer.take() {
                    Some(true) => {
                        self.start(rt, ui.ctx());
                        Outcome::Open
                    }
                    Some(false) => Outcome::Closed,
                    None if winui::escape_pressed(ui) => Outcome::Closed,
                    None => Outcome::Open,
                }
            },
        )
    }

    fn body(
        &mut self,
        ui: &mut Ui,
        o: Pos2,
        running: bool,
        failed: bool,
        answer: &mut Option<bool>,
    ) {
        let p = ui.painter().clone();
        let (ink, muted) = (winui::TEXT(), winui::MUTED());
        let w = 520.0;
        // The banner: MDM's own logo, what this is for, in one line, and why now.
        let extra = winui::banner(
            ui,
            &p,
            o,
            w,
            "helpers-banner",
            // i18n: the headline of the first start's helpers window
            crate::i18n::t("Download videos from YouTube and 1,000+ sites"),
            crate::i18n::t(
                "Two free helpers make it work. Get them now, and saving a video is one click away when you find one.",
            ),
        );
        // Everything below it moves down with a taller banner.
        let o = o + egui::vec2(0.0, extra);

        let ffmpeg_here = self
            .dir
            .as_deref()
            .and_then(mdm_tools::ffmpeg::installed)
            .is_some();
        let rows: [(Icon, &str, String); 3] = [
            (
                Icon::HelperVideo,
                // i18n: helper name and what it does (yt-dlp is a program's name)
                "yt-dlp: finds the video",
                crate::i18n::t(
                    "Reads a page and lists every quality the site offers, up to 4K, from YouTube, Vimeo and over a thousand other sites.",
                )
                .to_owned(),
            ),
            (
                Icon::HelperMerge,
                // i18n: helper name and what it does (ffmpeg is a program's name)
                "ffmpeg: joins picture and sound",
                if cfg!(windows) || ffmpeg_here {
                    crate::i18n::t(
                        "Sites send their best quality as separate video and audio. ffmpeg puts them together into one file that plays anywhere.",
                    )
                    .to_owned()
                } else {
                    crate::i18n::t(
                        "Sites send their best quality as separate video and audio, and ffmpeg joins them. On this system MDM uses yours: install ffmpeg with your package manager.",
                    )
                    .to_owned()
                },
            ),
            (
                Icon::HelperChecked,
                // i18n: a promise in the helpers window: the helpers are free, official and checked
                "Free, official and checked",
                crate::i18n::t(
                    "Both are open source. MDM gets them from their official releases with its own downloader and checks each file's signature before it runs. About 100 MB, once.",
                )
                .to_owned(),
            ),
        ];
        for (k, (icon, name, what)) in rows.iter().enumerate() {
            let y = 100.0 + k as f32 * 72.0;
            icon.image(true)
                .paint_at(ui, r(o, 22.0, y + 2.0, 32.0, 32.0));
            winui::text_font(
                &p,
                winui::tahoma_bold(),
                o.x + 68.0,
                o.y + y + 11.0,
                crate::i18n::t(name),
                ink,
            );
            winui::paragraph(
                &p,
                o.x + 68.0,
                o.y + y + 29.0,
                what,
                muted,
                o.x + w - 18.0,
                14.0,
            );
        }

        // After Yes: one bar for both, centred under the last section, and what it is doing.
        let mut y = 322.0;
        if let Some(job) = &self.job {
            let st = job.state.lock().unwrap();
            let f = job.progress.fraction().unwrap_or(0.0);
            let (fraction, line, color) = if let Some((name, error, _)) = &st.error {
                (
                    st.ready.len() as f32 / (st.ready.len() + 1) as f32,
                    crate::i18n::fmt(
                        // i18n: {name} = yt-dlp or ffmpeg; {error} = why
                        crate::i18n::t("{name} could not be downloaded: {error}"),
                        &[("name", name), ("error", error)],
                    ),
                    winui::ERROR(),
                )
            } else if st.done {
                let list = st
                    .ready
                    .iter()
                    .map(|(n, v)| format!("{n} {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    1.0,
                    crate::i18n::fmt(
                        // i18n: the helpers are downloaded, checked and ready; {list} = names and versions, e.g. "yt-dlp 2026.10.26, ffmpeg 9.0"
                        crate::i18n::t("Ready: {list}"),
                        &[("list", &list)],
                    ),
                    winui::SUCCESS(),
                )
            } else if let Some((name, k, total)) = st.current
                && f >= 1.0
            {
                (
                    k as f32 / total as f32,
                    crate::i18n::fmt(
                        // i18n: a downloaded helper's signature and checksum are being checked; {name} = yt-dlp or ffmpeg
                        crate::i18n::t("Checking {name} ({n} of {total})..."),
                        &[
                            ("name", name),
                            ("n", &crate::i18n::integer(k as u64)),
                            ("total", &crate::i18n::integer(total as u64)),
                        ],
                    ),
                    ink,
                )
            } else if let Some((name, k, total)) = st.current {
                (
                    ((k - 1) as f32 + f) / total as f32,
                    crate::i18n::fmt(
                        // i18n: {name} = yt-dlp or ffmpeg; {n} of {total} = which helper of how many; {percent} = how far, e.g. 45%
                        crate::i18n::t("Downloading {name} ({n} of {total})... {percent}"),
                        &[
                            ("name", name),
                            ("n", &crate::i18n::integer(k as u64)),
                            ("total", &crate::i18n::integer(total as u64)),
                            ("percent", &format!("{}%", (f * 100.0).floor())),
                        ],
                    ),
                    ink,
                )
            } else {
                (0.0, crate::i18n::t("Starting...").to_owned(), ink)
            };
            drop(st);
            let bar_w = 320.0;
            winui::meter(&p, r(o, (w - bar_w) / 2.0, 312.0, bar_w, 8.0), fraction);
            let room = w - 40.0;
            let mut more = 0.0;
            if failed && winui::text_width(&p, &line) > room {
                // A failure is read whole (#816): wrapped, the window growing by its lines past
                // the first.
                let n = winui::paragraph(
                    &p,
                    o.x + 20.0,
                    o.y + 340.0,
                    &line,
                    color,
                    o.x + w - 20.0,
                    16.0,
                );
                more = 16.0 * n.saturating_sub(1) as f32;
            } else {
                // Centred on screen (the same place in both directions); cut to the window.
                let shown = winui::ellipsize(&p, &winui::tahoma(), &line, room);
                let tw = winui::text_width(&p, &shown);
                let rc = winui::text_at(
                    &p,
                    winui::tahoma(),
                    (o.x + (w - tw) / 2.0).round(),
                    o.y + 340.0,
                    &shown,
                    color,
                );
                if shown != line {
                    winui::full_on_hover(ui, rc, "helpers-status", &line);
                }
            }
            crate::dialogs::grow_px(ui.ctx(), "helpers-error", more);
            y += 36.0 + more;
        }

        // The way back later, then the answer.
        winui::etched_h(&p, o.x + 16.0, o.y + y, w - 32.0);
        // What to do about the failure (#816), when it says.
        let advice = self.job.as_ref().and_then(|job| {
            let st = job.state.lock().unwrap();
            st.error.as_ref().and_then(|(_, _, a)| a.clone())
        });
        let later = if self.job.is_some() && running {
            crate::i18n::t("They keep downloading if you close this window.")
        } else if self.job.is_some() && !failed {
            crate::i18n::t(
                "All set: on a page with a video, use Tasks → Download video from a page.",
            )
        } else if let Some(advice) = &advice {
            advice.as_str()
        } else if self.job.is_some() {
            crate::i18n::t(
                "Check your connection, then try again, or get them later from the video window.",
            )
        } else {
            crate::i18n::t("Not now? Get them later from Tasks → Download video from a page.")
        };
        // Wrapped: the window grows by the lines past the first.
        let lines = winui::paragraph(
            &p,
            o.x + 16.0,
            o.y + y + 20.0,
            later,
            muted,
            o.x + w - 16.0,
            16.0,
        );
        crate::dialogs::grow_px(
            ui.ctx(),
            "helpers-advice",
            16.0 * lines.saturating_sub(1) as f32,
        );
        let y = y + 28.0 + 16.0 * lines.saturating_sub(1) as f32;
        if self.job.is_none() {
            // As wide as their labels in this language (#192), the row ending where it ends in
            // English.
            let [not_now_at, yes_at] = winui::button_row(
                &p,
                &[
                    ("Not now", 196.0, 90.0),
                    ("Yes, download them (recommended)", 292.0, 212.0),
                ],
                6.0,
                16.0,
                360.0,
            )[..] else {
                unreachable!("two buttons")
            };
            let not_now = winui::button(
                ui,
                r(o, not_now_at.0, y, not_now_at.1, 25.0),
                "Not now",
                ButtonKind::Normal,
                false,
                None,
            );
            // i18n: the first start's helpers window: download yt-dlp and ffmpeg (the recommended answer)
            let yes = winui::button(
                ui,
                r(o, yes_at.0, y, yes_at.1, 25.0),
                "Yes, download them (recommended)",
                ButtonKind::Accent,
                false,
                None,
            );
            if yes.clicked() || winui::enter_pressed(ui) {
                *answer = Some(true);
            } else if not_now.clicked() {
                *answer = Some(false);
            }
        } else {
            let [again_at, close_at] = winui::button_row(
                &p,
                &[("Try again", 318.0, 90.0), ("Close", 414.0, 90.0)],
                6.0,
                16.0,
                200.0,
            )[..] else {
                unreachable!("two buttons")
            };
            if failed && !running {
                let again = winui::button(
                    ui,
                    r(o, again_at.0, y, again_at.1, 25.0),
                    "Try again",
                    ButtonKind::Normal,
                    false,
                    None,
                );
                if again.clicked() {
                    *answer = Some(true);
                }
            }
            let close = winui::button(
                ui,
                r(o, close_at.0, y, close_at.1, 25.0),
                "Close",
                ButtonKind::Default,
                false,
                None,
            );
            if close.clicked() || winui::enter_pressed(ui) {
                *answer = Some(false);
            }
        }
    }
}

// ---- Video helpers: MDM's own copies or the person's (#51) ------------------------------------

use mdm_tools::chosen::Helper;

/// Helpers (Download video from a page or a gallery → Helpers...): for yt-dlp, ffmpeg and
/// gallery-dl (#58), where MDM takes it from (its own copy, or a program of the person's), and the
/// way to choose another: one copy of each for every program that uses them (#51).
pub struct Programs {
    preferences: mdm_tools::preferences::Preferences,
    tab: usize,
    config_editor: Option<(std::path::PathBuf, String)>,
    paths_only: bool,
    config_path: String,
    dir: Option<std::path::PathBuf>,
    /// The helpers as found (read again after each change, not every frame: it reads files).
    found: [Option<mdm_tools::Installed>; 4],
    last_detection: std::time::Instant,
    cookie_browser: String,
    cookie_profile: String,
    saved_cookies: Option<(String, String)>,
    /// A chosen program being checked (`--version`), and what it answered.
    checking: Option<(Helper, Answer)>,
    /// MDM's own yt-dlp or gallery-dl being downloaded in place of the system's (#452, #571),
    /// and how that went.
    installing: Option<(Helper, Answer)>,
    error: Option<String>,
}

const HELPERS: [Helper; 4] = [
    Helper::YtDlp,
    Helper::Ffmpeg,
    Helper::Deno,
    Helper::GalleryDl,
];

/// A check's answer, once it has one (Err: why the program wasn't taken, in words).
type Answer = Arc<Mutex<Option<Result<(), String>>>>;

impl Programs {
    fn preferences_body(&mut self, ui: &mut Ui, o: Pos2) -> Outcome<()> {
        let p = ui.painter().clone();
        for (label, y) in [
            ("Default quality", 65.0),
            ("Output container", 100.0),
            ("Video codec", 135.0),
            ("Preferred subtitle languages", 170.0),
        ] {
            winui::text(&p, o.x + 14.0, o.y + y, label, winui::TEXT());
        }
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 212.0, 52.0, 275.0, 22.0),
            &mdm_tools::preferences::QUALITIES,
            self.preferences.quality.min(8),
            true,
            "default-video-quality",
        ) {
            self.preferences.quality = i;
        }
        let ci = mdm_tools::preferences::CONTAINERS
            .iter()
            .position(|c| *c == self.preferences.container)
            .unwrap_or(0);
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 212.0, 87.0, 275.0, 22.0),
            &["Matroska (MKV)", "MPEG-4 (MP4)", "WebM"],
            ci,
            true,
            "default-video-container",
        ) {
            self.preferences.container = mdm_tools::preferences::CONTAINERS[i].into();
            if i == 2 && self.preferences.codec == 1 {
                self.preferences.codec = 2;
            }
        }
        let ids: Vec<usize> = (0..4)
            .filter(|i| self.preferences.container != "webm" || *i != 1)
            .collect();
        let names: Vec<&str> = ids
            .iter()
            .map(|i| mdm_tools::preferences::CODECS[*i])
            .collect();
        let selected = ids
            .iter()
            .position(|i| *i == self.preferences.codec)
            .unwrap_or(0);
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 212.0, 122.0, 275.0, 22.0),
            &names,
            selected,
            true,
            "default-video-codec",
        ) {
            self.preferences.codec = ids[i];
        }
        winui::edit(
            ui,
            r(o, 212.0, 157.0, 275.0, 23.0),
            &mut self.preferences.subtitle_languages,
            true,
            false,
        )
        .on_hover_text("Comma-separated yt-dlp language codes or patterns, for example en.*,si,ta");
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 194.0),
            &mut self.preferences.automatic_subtitles,
            "Include automatic captions when available",
            true,
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 220.0),
            &mut self.preferences.embed_subtitles,
            "Embed subtitles (converted to SRT)",
            true,
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 246.0),
            &mut self.preferences.embed_thumbnail,
            "Embed thumbnail when available",
            self.preferences.container != "webm",
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 272.0),
            &mut self.preferences.embed_chapters,
            "Embed chapters",
            true,
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 298.0),
            &mut self.preferences.embed_metadata,
            "Embed media metadata",
            true,
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 324.0),
            &mut self.preferences.embed_info_json,
            "Attach info JSON to Matroska files",
            true,
        );
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 350.0),
            &mut self.preferences.use_config,
            "Use a configuration file by default",
            true,
        );
        winui::text(
            &p,
            o.x + 14.0,
            o.y + 385.0,
            "Filename template",
            winui::TEXT(),
        );
        winui::edit(ui,r(o,14.0,394.0,473.0,23.0),&mut self.preferences.filename_template,true,false).on_hover_text("Supported fields: %(title)s, %(id)s, %(uploader)s, %(upload_date)s and %(ext)s. Folder paths are not allowed.");
        winui::checkbox(
            ui,
            egui::pos2(o.x + 14.0, o.y + 435.0),
            &mut self.preferences.remote_ejs,
            "Allow yt-dlp EJS components from GitHub",
            true,
        );
        if let Some(e) = &self.error {
            winui::fit(
                ui,
                &p,
                o.x + 14.0,
                o.y + 475.0,
                e,
                winui::ERROR(),
                o.x + 487.0,
                "yt-preferences-error",
            );
        }
        if winui::button(
            ui,
            r(o, 328.0, 486.0, 77.0, 22.0),
            "Save",
            ButtonKind::Default,
            false,
            None,
        )
        .clicked()
        {
            self.error = self.preferences.save().err().map(|e| e.to_string());
        }
        if winui::button(
            ui,
            r(o, 413.0, 486.0, 77.0, 22.0),
            "Close",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
            || winui::escape_pressed(ui)
        {
            return Outcome::Closed;
        }
        Outcome::Open
    }

    pub fn paths() -> Self {
        let mut p = Self::new();
        p.paths_only = true;
        if let Some(cookies) = mdm_tools::proc::browser_cookies() {
            p.cookie_browser = cookies.browser;
            p.cookie_profile = cookies.profile.unwrap_or_default();
        }
        p
    }
    pub fn new() -> Self {
        let mut p = Programs {
            preferences: mdm_tools::preferences::Preferences::read(),
            tab: 0,
            config_editor: None,
            paths_only: false,
            config_path: mdm_tools::config::path()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            dir: mdm_tools::tools_dir(),
            found: Default::default(),
            last_detection: std::time::Instant::now(),
            cookie_browser: String::new(),
            cookie_profile: String::new(),
            saved_cookies: None,
            checking: None,
            installing: None,
            error: None,
        };
        p.look();
        p
    }

    pub fn take_saved_cookies(&mut self) -> Option<(String, String)> {
        self.saved_cookies.take()
    }

    fn look(&mut self) {
        self.last_detection = std::time::Instant::now();
        let Some(dir) = self.dir.as_deref() else {
            return;
        };
        self.found = [
            mdm_tools::installed(dir),
            mdm_tools::ffmpeg::installed(dir),
            mdm_tools::deno::installed(dir),
            mdm_tools::gallery::installed(dir),
        ];
    }

    /// MDM's own yt-dlp or gallery-dl in place of the system's (#452, #571): downloaded and
    /// checked as usual, from then on the one used (`mdm_tools::installed`,
    /// `mdm_tools::gallery::installed`). It goes on if the window is closed.
    fn install_own(
        &mut self,
        h: Helper,
        dir: std::path::PathBuf,
        rt: &tokio::runtime::Handle,
        ctx: &egui::Context,
    ) {
        let slot: Answer = Arc::new(Mutex::new(None));
        let (out, wake) = (slot.clone(), ctx.clone());
        let net = crate::settings::own_net();
        rt.spawn(async move {
            let res = if h == Helper::GalleryDl {
                mdm_tools::gallery::install_latest(&dir, &net, None).await
            } else if h == Helper::Ffmpeg {
                mdm_tools::ffmpeg::install_latest(&dir, &net, None).await
            } else if h == Helper::Deno {
                mdm_tools::deno::install_latest(&dir, &net).await
            } else {
                mdm_tools::install_latest(&dir, &net, None).await
            };
            *out.lock().unwrap() = Some(res.map(|_| ()).map_err(|e| crate::errors::tools(&e)));
            wake.request_repaint();
        });
        self.installing = Some((h, slot));
        self.error = None;
    }

    /// Open until Close.
    pub fn show(&mut self, ctx: &egui::Context, rt: &tokio::runtime::Handle) -> Outcome<()> {
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        if self.last_detection.elapsed() >= std::time::Duration::from_secs(2) {
            self.look();
        }
        let answered = self
            .checking
            .as_ref()
            .and_then(|(_, slot)| slot.lock().unwrap().take());
        if let Some(res) = answered {
            self.error = res.err();
            self.checking = None;
            self.look();
        }
        let installed = self
            .installing
            .as_ref()
            .and_then(|(_, slot)| slot.lock().unwrap().take());
        if let Some(res) = installed {
            self.error = res.err();
            self.installing = None;
            self.look();
        }
        for h in HELPERS {
            if let Some(path) = winui::picked(ctx, &format!("helper-{}", h.name()))
                && let Some(dir) = self.dir.clone()
            {
                let slot = Arc::new(Mutex::new(None));
                let (out, wake) = (slot.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let res = mdm_tools::chosen::choose(&dir, h, &path)
                        .map(|_| ())
                        .map_err(|e| crate::errors::tools(&e));
                    *out.lock().unwrap() = Some(res);
                    wake.request_repaint();
                });
                self.checking = Some((h, slot));
                self.error = None;
            }
        }
        if let Some((path, text)) = &mut self.config_editor {
            let out = window(
                ctx,
                "ytdlp-config-editor",
                "Edit yt-dlp configuration",
                [620.0, 410.0],
                false,
                winui::BG(),
                |ui, o| {
                    winui::text(
                        ui.painter(),
                        o.x + 12.0,
                        o.y + 20.0,
                        &path.display().to_string(),
                        winui::TEXT(),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("config-text-scroll")
                        .max_height(305.0)
                        .show_viewport(ui, |ui, _| {
                            ui.put(
                                r(o, 12.0, 32.0, 596.0, 300.0),
                                egui::TextEdit::multiline(text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(590.0),
                            );
                        });
                    winui::text(
                        ui.painter(),
                        o.x + 12.0,
                        o.y + 355.0,
                        "Orbit manages transfers; external downloaders and command hooks are ignored.",
                        winui::MUTED(),
                    );
                    if winui::button(
                        ui,
                        r(o, 446.0, 376.0, 77.0, 22.0),
                        "Save",
                        ButtonKind::Default,
                        false,
                        None,
                    )
                    .clicked()
                    {
                        return Outcome::Done(());
                    }
                    if winui::button(
                        ui,
                        r(o, 531.0, 376.0, 77.0, 22.0),
                        "Cancel",
                        ButtonKind::Normal,
                        false,
                        None,
                    )
                    .clicked()
                        || winui::escape_pressed(ui)
                    {
                        return Outcome::Closed;
                    }
                    Outcome::Open
                },
            );
            if matches!(out, Outcome::Done(())) {
                let result = (|| -> std::io::Result<()> {
                    mdm_tools::config::validate_text(text)?;
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let tmp = path.with_extension("conf.orbit.tmp");
                    std::fs::write(&tmp, text.as_bytes())?;
                    std::fs::rename(tmp, &*path)?;
                    if let Some(dir) = &self.dir {
                        mdm_tools::chosen::set_config_file(dir, Some(path))
                            .map_err(std::io::Error::other)?;
                    }
                    Ok(())
                })();
                self.error = result.err().map(|e| e.to_string());
                if self.error.is_none() {
                    self.config_path = path.display().to_string();
                    self.config_editor = None;
                }
            } else if matches!(out, Outcome::Closed) {
                self.config_editor = None;
            }
        }
        window(
            ctx,
            "video-helpers",
            // i18n: window title: where MDM takes its helper programs (yt-dlp, ffmpeg, gallery-dl) from
            if self.paths_only {
                "yt-dlp settings"
            } else {
                "Helpers"
            },
            [500.0, if self.paths_only { 605.0 } else { 292.0 }],
            false,
            winui::BG(),
            |ui, o| self.body(ui, o, rt),
        )
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2, rt: &tokio::runtime::Handle) -> Outcome<()> {
        let p = ui.painter().clone();
        if self.paths_only {
            for (i, label) in ["Download defaults", "Programs and configuration"]
                .iter()
                .enumerate()
            {
                if winui::button(
                    ui,
                    r(o, 14.0 + i as f32 * 238.0, 9.0, 235.0, 24.0),
                    label,
                    if self.tab == i {
                        ButtonKind::Default
                    } else {
                        ButtonKind::Normal
                    },
                    false,
                    None,
                )
                .clicked()
                {
                    self.tab = i;
                }
            }
            if self.tab == 0 {
                return self.preferences_body(ui, o);
            }
        }
        let o = if self.paths_only {
            o + egui::vec2(0.0, 40.0)
        } else {
            o
        };

        let shown = if self.paths_only {
            [0, 1, 2]
        } else {
            [0, 1, 3]
        };
        for (k, i) in shown.into_iter().enumerate() {
            let h = HELPERS[i];
            let y = o.y + 8.0 + k as f32 * 78.0;
            winui::text_font(
                &p,
                winui::tahoma_bold(),
                o.x + 14.0,
                y + 16.0,
                h.name(),
                winui::TEXT(),
            );
            let found = self.found[i].as_ref();
            let chosen = self.dir.as_deref().and_then(|d| {
                if h == Helper::Deno {
                    mdm_tools::deno::explicit(d)
                } else {
                    mdm_tools::chosen::chosen(d, h)
                }
            });
            // One of the person's, maybe passed over (a gallery-dl too old, #236).
            let mine = chosen.is_some();
            let too_old = chosen
                .as_ref()
                .filter(|c| h == Helper::GalleryDl && mdm_tools::gallery::too_old(&c.version));
            let checking = self.checking.as_ref().is_some_and(|(c, _)| *c == h);
            let downloading = self.installing.as_ref().is_some_and(|(i, _)| *i == h);
            // Whether MDM downloads a copy of its own on this system and processor: yt-dlp
            // almost everywhere, gallery-dl not on macOS nor Linux on ARM, ffmpeg on 64-bit
            // Windows only (BtbN builds none for 32-bit Windows).
            let has_own = match h {
                Helper::YtDlp => mdm_tools::ytdlp_asset().is_some(),
                Helper::GalleryDl => mdm_tools::gallery::asset().is_some(),
                Helper::Ffmpeg => mdm_tools::ffmpeg::available(),
                Helper::Deno => mdm_tools::deno::available(),
            };
            // The system's copy may be the person's to keep up to date: MDM's own can replace it
            // (#452, #571), where MDM has one for this system and processor.
            let status = match (too_old, found) {
                _ if downloading => {
                    // i18n: Video helpers: MDM's own yt-dlp is being downloaded
                    crate::i18n::t("Downloading MDM's own copy...").to_owned()
                }
                _ if checking => {
                    // i18n: Video helpers: a program just chosen is being checked
                    crate::i18n::t("Checking the program...").to_owned()
                }
                (Some(old), _) => crate::i18n::fmt(
                    // i18n: Video helpers: the person's own gallery-dl is too old and not used; {version} = its version, {oldest} = the oldest MDM can use
                    crate::i18n::t(
                        "Your own program ({version}) is too old: MDM needs {oldest} or newer",
                    ),
                    &[
                        ("version", &old.version),
                        ("oldest", mdm_tools::gallery::OLDEST),
                    ],
                ),
                (None, Some(i)) if mine && !i.version.is_empty() => crate::i18n::fmt(
                    // i18n: Video helpers: {path} = the program the person chose, {version} = its version
                    crate::i18n::t("Your own program: {path} ({version})"),
                    &[
                        ("path", &crate::i18n::ltr(&i.exe.display().to_string())),
                        ("version", &i.version),
                    ],
                ),
                (None, Some(i)) if i.is_system() || mine => crate::i18n::fmt(
                    // i18n: Video helpers: {path} = the system's yt-dlp, ffmpeg or gallery-dl
                    crate::i18n::t("Your system's: {path}"),
                    &[("path", &crate::i18n::ltr(&i.exe.display().to_string()))],
                ),
                (None, Some(i)) => crate::i18n::fmt(
                    // i18n: Video helpers: {version} = the version of MDM's own copy
                    crate::i18n::t("Orbit copy {version}: {path}"),
                    &[
                        ("version", &i.version),
                        ("path", &crate::i18n::ltr(&i.exe.display().to_string())),
                    ],
                ),
                // Linux and macOS as before: there the system's package manager (or Homebrew)
                // installs what MDM has no copy of, and MDM finds it.
                // i18n: Video helpers: no copy of this helper yet
                (None, None) if has_own || !cfg!(windows) => {
                    crate::i18n::t("Not installed yet").to_owned()
                }
                // i18n: Video helpers: MDM can't download this helper on this Windows (ffmpeg on 32-bit Windows); "Use another program" is the button below
                (None, None) => crate::i18n::t(
                    "MDM has no copy for this system: install it, or use another program",
                )
                .to_owned(),
            };
            winui::fit(
                ui,
                &p,
                o.x + 14.0,
                y + 36.0,
                &status,
                winui::MUTED(),
                o.x + 486.0,
                ("helper", k),
            );
            let busy = self.checking.is_some() || self.installing.is_some();
            let action = if mine {
                "Use detected / Orbit copy"
            } else if found.is_none() && has_own {
                "Download and install"
            } else if found.is_none() && h == Helper::Deno {
                "Deno installation page"
            } else {
                "Detect again"
            };
            // As wide as their labels in this language (#192), side by side from the left.
            let [other_at, own_at] = winui::button_row_from_left(
                &p,
                &[
                    ("Use another program...", 14.0, 180.0),
                    (action, 202.0, 180.0),
                ],
                8.0,
                486.0,
                260.0,
            )[..] else {
                unreachable!("two buttons")
            };
            if winui::button(
                ui,
                r(o, other_at.0, y - o.y + 46.0, other_at.1, 21.0),
                // i18n: Video helpers: button: pick a program (yt-dlp or ffmpeg) already on the computer
                "Use another program...",
                if busy {
                    ButtonKind::Disabled
                } else {
                    ButtonKind::Normal
                },
                false,
                None,
            )
            .clicked()
            {
                // i18n: file picker title; {name} = yt-dlp or ffmpeg
                let title =
                    crate::i18n::fmt(crate::i18n::t("Choose {name}"), &[("name", h.name())]);
                winui::start_pick(ui.ctx(), &format!("helper-{}", h.name()), move || {
                    crate::winui::file_dialog(&title).pick_file()
                });
            }
            if winui::button(
                ui,
                r(o, own_at.0, y - o.y + 46.0, own_at.1, 21.0),
                // i18n: Video helpers: button: go back to the copy MDM downloads and checks itself
                action,
                if !busy {
                    ButtonKind::Normal
                } else {
                    ButtonKind::Disabled
                },
                false,
                None,
            )
            .clicked()
                && let Some(dir) = self.dir.clone()
            {
                if mine {
                    let result = if h == Helper::Deno {
                        mdm_tools::deno::reset(&dir)
                    } else {
                        mdm_tools::chosen::forget(&dir, h)
                    };
                    self.error = result.err().map(|e| crate::errors::tools(&e));
                    self.look();
                } else if found.is_none() && has_own {
                    self.look();
                    if self.found[i].is_none() {
                        self.install_own(h, dir, rt, ui.ctx());
                    }
                } else if found.is_none() && h == Helper::Deno {
                    let _ = open::that_detached(mdm_tools::deno::INSTALL_PAGE);
                } else {
                    self.look();
                }
            }
            if self.paths_only || k + 1 < shown.len() {
                winui::etched_h(&p, o.x + 14.0, o.y + 82.0 + k as f32 * 78.0, 472.0);
            }
        }
        if let Some(e) = &self.error {
            winui::fit(
                ui,
                &p,
                o.x + 14.0,
                o.y + if self.paths_only { 512.0 } else { 254.0 },
                e,
                winui::ERROR(),
                o.x + 400.0,
                "helper-error",
            );
        }
        if self.paths_only {
            let o = o + egui::vec2(0.0, 78.0);
            winui::text(
                &p,
                o.x + 14.0,
                o.y + 181.0,
                "yt-dlp configuration file (optional)",
                winui::TEXT(),
            );
            winui::edit(
                ui,
                r(o, 14.0, 190.0, 386.0, 23.0),
                &mut self.config_path,
                true,
                false,
            );
            if winui::button(
                ui,
                r(o, 410.0, 191.0, 77.0, 21.0),
                "Browse",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
            {
                winui::start_pick(ui.ctx(), "ytdlp-config", || {
                    crate::winui::file_dialog("Choose yt-dlp configuration")
                        .add_filter("yt-dlp configuration", &["conf", "txt"])
                        .pick_file()
                });
            }
            if let Some(file) = winui::picked(ui.ctx(), "ytdlp-config") {
                self.config_path = file.display().to_string();
            }
            if winui::button(
                ui,
                r(o, 14.0, 222.0, 130.0, 23.0),
                "Apply configuration",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
            {
                if let Some(dir) = &self.dir {
                    let file = (!self.config_path.trim().is_empty())
                        .then(|| std::path::Path::new(self.config_path.trim()));
                    self.error = mdm_tools::chosen::set_config_file(dir, file)
                        .err()
                        .map(|e| e.to_string());
                }
            }
            if winui::button(
                ui,
                r(o, 154.0, 222.0, 188.0, 23.0),
                "Use user-folder yt-dlp.conf",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
            {
                if let Some(dir) = &self.dir {
                    self.error = mdm_tools::chosen::set_config_file(dir, None)
                        .err()
                        .map(|e| e.to_string());
                }
                self.config_path = mdm_tools::config::path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
            }
            winui::text(
                &p,
                o.x + 14.0,
                o.y + 272.0,
                "Select configuration-file mode in the video download window.",
                winui::TEXT(),
            );
            winui::text(
                &p,
                o.x + 14.0,
                o.y + 290.0,
                "Orbit handles transfers; external downloader commands are ignored.",
                winui::TEXT(),
            );
            if winui::button(
                ui,
                r(o, 352.0, 222.0, 135.0, 23.0),
                "Edit configuration...",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
            {
                let path = if self.config_path.trim().is_empty() {
                    std::env::var_os("USERPROFILE")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_default()
                        .join("yt-dlp.conf")
                } else {
                    std::path::PathBuf::from(self.config_path.trim())
                };
                match std::fs::read_to_string(&path) {
                    Ok(text)=>self.config_editor=Some((path,text)),
                    Err(e) if e.kind()==std::io::ErrorKind::NotFound=>self.config_editor=Some((path,"# yt-dlp preferences for Orbit Download Manager\n-f \"bestvideo[height<=720][vcodec^=avc1]+bestaudio / best[height<=720]\"\n--merge-output-format mkv\n--embed-thumbnail\n--embed-metadata\n--embed-info-json\n--convert-subs srt\n".replace("\\n","\n"))),
                    Err(e)=>self.error=Some(e.to_string()),
                }
            }
            winui::text(
                &p,
                o.x + 14.0,
                o.y + 330.0,
                "Cookies from browser",
                winui::TEXT(),
            );
            let browsers = mdm_tools::proc::cookie_browsers();
            let names: Vec<_> = std::iter::once("Off")
                .chain(browsers.iter().map(|b| crate::videodlg::browser_label(b)))
                .collect();
            let selected = browsers
                .iter()
                .position(|b| *b == self.cookie_browser)
                .map_or(0, |i| i + 1);
            if let Some(i) = winui::combo_light(
                ui,
                r(o, 212.0, 315.0, 275.0, 24.0),
                &names,
                selected,
                true,
                "ytdlp-cookie-browser",
            ) {
                self.cookie_browser = i.checked_sub(1).map_or("", |i| browsers[i]).to_owned();
            }
            winui::text(
                &p,
                o.x + 14.0,
                o.y + 363.0,
                "Profile (optional):",
                winui::TEXT(),
            );
            winui::edit(
                ui,
                r(o, 212.0, 348.0, 275.0, 24.0),
                &mut self.cookie_profile,
                !self.cookie_browser.is_empty(),
                false,
            );
            winui::fit(
                ui,
                &p,
                o.x + 14.0,
                o.y + 391.0,
                crate::i18n::t(
                    "Used when the Orbit extension did not supply cookies for the page.",
                ),
                winui::MUTED(),
                o.x + 487.0,
                "ytdlp-cookie-note",
            );
            if winui::button(
                ui,
                r(o, 328.0, 451.0, 77.0, 21.0),
                "Save",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
            {
                self.saved_cookies = Some((
                    self.cookie_browser.clone(),
                    self.cookie_profile.trim().to_owned(),
                ));
            }
        }
        let close = winui::button(
            ui,
            r(
                o,
                413.0,
                if self.paths_only { 529.0 } else { 260.0 },
                77.0,
                21.0,
            ),
            "Close",
            ButtonKind::Default,
            false,
            None,
        );
        if close.clicked() || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
            Outcome::Done(())
        } else {
            Outcome::Open
        }
    }
}
