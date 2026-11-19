//! Tasks → Download video from a page… (M9, ours: IDM's video panel is fed by its browser
//! module, which comes with M8). Paste a page, look it up with yt-dlp, pick a quality, choose
//! where it goes. yt-dlp is installed (verified, D-011) from here on first use, and ffmpeg (the
//! LGPL build) when the best qualities need merging. It follows the app's mode, like Add torrent.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Pos2, Ui, pos2};
use mdm_tools::declared::Declared;
use mdm_tools::resolve::{Choice, Looked, SubChoice, Video};
use mdm_tools::video::{ALL_UPLOADERS, Subs};

use crate::dialogs::{Outcome, window};
use crate::model::Category;
use crate::winui::{self, ButtonKind, r, text};

type Slot<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// What Download Now or Later adds: a video, or a playlist's (#69).
pub enum VideoOut {
    One(VideoAdd),
    Configured {
        page: String,
        context: mdm_ipc::Command,
        profile: mdm_tools::config::Profile,
        start_now: bool,
    },
    Many(crate::playlistdlg::PlaylistAdd),
}

/// What OK adds.
pub struct VideoAdd {
    pub ytdlp: Option<mdm_tools::config::Profile>,
    pub page: url::Url,
    pub selector: String,
    pub dest: PathBuf,
    /// The category it is listed under: Music for a sound-only choice (#611), else Video.
    pub category: Category,
    pub size: Option<u64>,
    pub start_now: bool,
    /// What the browser's request for the page carried (its button), for yt-dlp to ask the
    /// same; none for an address typed or edited here.
    pub context: mdm_ipc::Command,
    /// The subtitles to save with it (#68; none for a sound-only choice).
    pub subs: Subs,
    /// The Subtitles choice to remember for the next video (settings.video_subs); none when
    /// the page had no subtitles to choose from (the last real choice stays).
    pub remember: Option<Subs>,
    /// The browser's quality menu's look-up the choice was made from, for the download's first
    /// run (videolookup.rs); none from this window, whose download looks the page up itself.
    pub preloaded: Option<mdm_tools::video::Preloaded>,
    /// The user typed the file name (#1102): kept as typed; else it is the page's title, which
    /// is shortened to fit the path limits.
    pub named: bool,
    /// The video's length (whole seconds) as looked up, for the duplicate check
    /// ([`whole_seconds`], `model::VideoSpec::duration`).
    pub duration: Option<u32>,
}

/// A looked-up length (`resolve::Video::duration`, seconds) as a video download keeps it: whole
/// seconds; none under a second, or not a number.
pub fn whole_seconds(length: Option<f64>) -> Option<u32> {
    length
        .filter(|s| s.is_finite() && *s >= 1.0 && *s < f64::from(u32::MAX))
        .map(|s| s.round() as u32)
}

/// Which helper is being installed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    YtDlp,
    Ffmpeg,
}

pub struct VideoDialog {
    preferences: mdm_tools::preferences::Preferences,
    use_config: bool,
    advanced: bool,
    format_code: Option<String>,
    name_edited: bool,
    tool_check: std::time::Instant,
    thumbnail: bool,
    preferred_subtitles: bool,
    kept: Option<mdm_tools::video::InfoSlot>,
    page: String,
    /// The browser's request context for `page` as it came (VideoAdd::context).
    context: mdm_ipc::Command,
    from_browser: String,
    /// Save to.
    folder: String,
    /// The Video and Music categories' folders MDM puts in Save to (#611): Music's while a
    /// sound-only choice is chosen, as long as Save to holds one of them (a folder typed or
    /// picked stays). No Music folder: the Music category was deleted, all goes as video.
    video_dir: String,
    music_dir: Option<String>,
    /// Save to was last set for a sound-only choice.
    sound: bool,
    name: String,
    lookup: Option<Slot<(Looked, Option<Declared>)>>,
    /// The video the page looked up declares its own (#1076): what the download is added with
    /// (its address, the page as its Referer), while the address is still the one looked up.
    declared: Option<Declared>,
    /// The yt-dlp and the browser's context the look-up ran with (a list is listed with them).
    looking: Option<(PathBuf, mdm_tools::proc::FromBrowser)>,
    /// The page is a playlist or a channel: its videos (#69).
    list: Option<crate::playlistdlg::PlaylistView>,
    /// Adding that many was answered Yes: Download Now (true) or Later goes ahead.
    confirmed: Option<bool>,
    install: Option<(Tool, Slot<mdm_tools::Installed>)>,
    /// The helpers found (read again when an install ends, not every frame: it reads files).
    tools: Option<Tools>,
    /// The address the choices were looked up for (an edited one needs Look up again).
    looked_up: String,
    video: Option<(Video, Vec<Choice>)>,
    chosen: usize,
    /// First choice shown (the list scrolls by whole rows).
    /// The choices list's scroll and selection.
    rows: winui::RowList,
    error: Option<String>,
    focused: bool,
    /// Download Now (true) or Later, once chosen.
    start_now: Option<bool>,
    /// Asking to create a missing folder.
    asking: crate::dialogs::FolderGate<()>,
    /// Helpers...: where yt-dlp and ffmpeg come from (#51).
    helpers: Option<crate::helpers::Programs>,
    /// The looked-up video's subtitle languages (#68) and the one chosen in the Subtitles list
    /// (0: none, then "All the uploader's" when offered, then `subs` in order).
    subs: Vec<SubChoice>,
    sub_pick: usize,
    /// Put the subtitles into the video rather than beside it.
    embed: bool,
    /// Embed chapters (#827; settings.video_chapters).
    chapters: bool,
    /// The last video's choice (settings.video_subs), applied to each video looked up.
    pref: Subs,
    /// The clipboard's page address, read off the window's thread (#661).
    clip: crate::clipboard::Prefill,
}

const WIDTH: f32 = 680.0;
const HEIGHT: f32 = 821.0;
const LEFT: f32 = 37.0;
const FIELD_X: f32 = 189.0;
const RIGHT: f32 = WIDTH - LEFT;

/// What is installed in the tools folder: (folder, yt-dlp, ffmpeg).
type Tools = (
    Option<PathBuf>,
    Option<mdm_tools::Installed>,
    Option<mdm_tools::Installed>,
);

/// Keep yt-dlp current: video sites change often and an old yt-dlp is the usual reason a page
/// stops working. Once per session, when the video window opens and yt-dlp is installed, the
/// newest release is fetched in the background (mdm_tools::install_latest: signed by yt-dlp's
/// developers, never older, switched atomically, the previous version kept). Quiet: a failure
/// leaves the installed version in use. A system copy is left to its package manager.
pub fn refresh_ytdlp(rt: &tokio::runtime::Handle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    let Some(dir) = mdm_tools::tools_dir() else {
        return;
    };
    if !mdm_tools::refreshable(&dir) || DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    rt.spawn(async move {
        if let Err(e) = mdm_tools::install_latest(&dir, &crate::settings::own_net(), None).await {
            eprintln!("yt-dlp update check: {e}"); // not translated: a log line
        }
    });
}

fn tools() -> Tools {
    let dir = mdm_tools::tools_dir();
    let ytdlp = dir.as_deref().and_then(mdm_tools::installed);
    let ffmpeg = dir.as_deref().and_then(mdm_tools::ffmpeg::installed);
    (dir, ytdlp, ffmpeg)
}

/// Keep the default video container from remuxing an audio-only selection. A remux
/// explicitly requested in a configuration file still takes precedence.
pub(crate) fn audio_profile(mut profile: mdm_tools::config::Profile) -> mdm_tools::config::Profile {
    let tail = profile
        .args
        .strip_prefix(profile.base_args.as_slice())
        .map(<[_]>::to_vec);
    let mut base = Vec::new();
    let mut args = profile.base_args.iter();
    while let Some(arg) = args.next() {
        if arg == "--remux-video" {
            args.next();
        } else {
            base.push(arg.clone());
        }
    }
    if let Some(tail) = tail {
        profile.args = base.iter().cloned().chain(tail).collect();
    }
    profile.base_args = base;
    profile
}

impl VideoDialog {
    /// Opened from the menu: a video page's address on the clipboard is taken (read off the
    /// window's thread, put in the box on the frame it comes).
    pub fn new(folder: PathBuf) -> Self {
        VideoDialog {
            clip: crate::clipboard::Prefill::ask(),
            ..Self::blank(folder)
        }
    }

    /// The clipboard's text as the page, when it is a web address and the box is still empty
    /// (nothing typed meanwhile).
    fn fill_from_clipboard(&mut self, clip: &str) {
        let clip = clip.trim();
        if self.page.is_empty()
            && url::Url::parse(clip).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
        {
            self.page = clip.to_owned();
        }
    }

    /// No address yet, saving into `folder` (the Video category's).
    fn blank(folder: PathBuf) -> Self {
        let folder = folder.display().to_string();
        let preferences = mdm_tools::preferences::Preferences::read();
        let quality = preferences.quality.min(8);
        VideoDialog {
            use_config: preferences.use_config,
            thumbnail: preferences.embed_thumbnail,
            chapters: preferences.embed_chapters,
            embed: preferences.embed_subtitles,
            preferences,
            advanced: false,
            format_code: None,
            name_edited: false,
            tool_check: std::time::Instant::now(),
            preferred_subtitles: true,
            kept: None,
            page: String::new(),
            context: Default::default(),
            from_browser: String::new(),
            video_dir: folder.clone(),
            music_dir: None,
            sound: false,
            folder,
            name: String::new(),
            lookup: None,
            declared: None,
            looking: None,
            list: None,
            confirmed: None,
            install: None,
            tools: None,
            looked_up: String::new(),
            video: None,
            chosen: quality,
            rows: Default::default(),
            error: None,
            focused: false,
            start_now: None,
            asking: Default::default(),
            helpers: None,
            subs: Vec::new(),
            sub_pick: 0,
            pref: Subs::default(),
            clip: Default::default(),
        }
    }

    /// Embed chapters ticked or not (settings.video_chapters, #827).
    pub fn with_chapters(mut self, on: bool) -> Self {
        self.chapters = on;
        self
    }

    /// Offer `pref` (the last video's subtitles, settings.video_subs) for the videos looked up.
    pub fn with_subs(mut self, pref: Subs) -> Self {
        self.preferred_subtitles = false;
        self.embed = pref.embed;
        self.pref = pref;
        self
    }

    /// Save to switches to `dir`, the Music category's folder, while a sound-only choice is
    /// chosen (#611); None (the Music category was deleted): sound goes as video does.
    pub fn with_music_folder(mut self, dir: Option<PathBuf>) -> Self {
        self.music_dir = dir.map(|d| d.display().to_string());
        self
    }

    /// Opened for `page` (Add URL was given a video's page, or the browser's button was
    /// clicked with `context`): lets the user choose configuration-file mode before lookup.
    pub fn for_page(folder: PathBuf, page: &str, mut context: mdm_ipc::Command) -> Self {
        let own = mdm_tools::pages::video_address(page);
        // An embedded player taken to the video's own page (#359): the site that embedded it
        // isn't that page's referer, and Dailymotion refuses its own page asked with one (403).
        if own != page {
            context.referer = None;
        }
        let page = own;
        VideoDialog {
            page: page.clone(),
            from_browser: page,
            context,
            ..Self::blank(folder)
        }
    }

    /// The browser's context, while the address is still the one it came with.
    fn context(&self) -> mdm_ipc::Command {
        if self.page.trim() == self.from_browser {
            self.context.clone()
        } else {
            mdm_ipc::Command::default()
        }
    }

    fn preset_choices(&self, video: Option<&Video>) -> Vec<Choice> {
        mdm_tools::preferences::QUALITIES
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let height = mdm_tools::preferences::HEIGHTS[index];
                let size = video.and_then(|v| {
                    v.formats
                        .iter()
                        .filter(|f| {
                            f.video && height.is_none_or(|h| f.height.is_some_and(|x| x <= h))
                        })
                        .max_by_key(|f| f.height)
                        .and_then(|f| f.size)
                });
                Choice {
                    label: label.to_string(),
                    selector: self.preferences.selector(index),
                    ext: self.preferences.container.clone(),
                    height,
                    size,
                    approx: true,
                    ..Default::default()
                }
            })
            .collect()
    }
    fn choices(&self, video: &Video, merge: bool) -> Vec<Choice> {
        if self.advanced {
            mdm_tools::resolve::choices(video, merge)
        } else {
            self.preset_choices(Some(video))
        }
    }
    fn manual_profile(&self) -> Result<mdm_tools::config::Profile, String> {
        let mut p = self.preferences.clone();
        p.embed_thumbnail = self.thumbnail && p.container != "webm";
        p.embed_chapters = self.chapters;
        p.embed_subtitles = self.embed;
        if !self.preferred_subtitles {
            let chosen = self.chosen_subs();
            p.subtitle_languages = chosen.langs.join(",");
            p.automatic_subtitles = chosen.automatic;
        }
        p.validate().map_err(|e| e.to_string())?;
        p.profile(false).map_err(|e| e.to_string())
    }

    /// The choices for a looked-up video: merged qualities when ffmpeg is there.
    fn set_video(&mut self, mut v: Video) {
        // No page reader for this site: the page's own title rather than a hash (#345).
        if self.page.trim() == self.from_browser
            && let Some(title) = page_title(&self.context, &v, &self.page)
        {
            v.title = title.to_owned();
        }
        let merge = self.tools.get_or_insert_with(tools).2.is_some();
        let choices = self.choices(&v, merge);
        // The best, as `video-get` takes it by default (#1240).
        self.chosen = if self.advanced {
            mdm_tools::resolve::pick(&choices, None).map_or(0, |p| p.index)
        } else {
            self.preferences
                .quality
                .min(choices.len().saturating_sub(1))
        };
        self.rows = Default::default();
        self.error = choices
            .is_empty()
            .then(|| crate::i18n::t("No video this can download on that page").into());
        self.subs = mdm_tools::resolve::sub_choices(&v, crate::i18n::code());
        self.sub_pick = pick_for(&self.subs, &self.pref);
        self.video = Some((v, choices));
        self.name_from_choice();
    }

    /// The Subtitles list's choice as yt-dlp is asked for it.
    fn chosen_subs(&self) -> Subs {
        if self.preferred_subtitles {
            return Subs {
                langs: self
                    .preferences
                    .subtitle_languages
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                automatic: self.preferences.automatic_subtitles,
                embed: self.embed,
            };
        }
        subs_at(&self.subs, self.sub_pick, self.embed)
    }

    /// A looked-up video with subtitles in four languages, without yt-dlp or the network
    /// (`/show video-subs`: the window's check in every language, its screenshots).
    pub fn show_sample(&mut self) {
        // Its 4K in AV1 and VP9, HDR and SDR, each its own line (#755); 720p sized only
        // about (#801).
        const SAMPLE: &str = r#"{"title": "A conference talk about download managers, recorded in Lisbon",
            "duration": 1830, "language": "en", "formats": [
            {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a.40.2", "quality": 3, "tbr": 129.5, "filesize": 29600000},
            {"format_id": "251", "ext": "webm", "vcodec": "none", "acodec": "opus", "quality": 3, "tbr": 121.8, "filesize": 27900000},
            {"format_id": "18", "ext": "mp4", "height": 360, "fps": 30, "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "filesize": 61000000},
            {"format_id": "701", "ext": "mp4", "height": 2160, "fps": 60, "vcodec": "av01.0.13M.10", "acodec": "none", "dynamic_range": "HDR10", "filesize": 1951000000},
            {"format_id": "337", "ext": "webm", "height": 2160, "fps": 60, "vcodec": "vp09.02.51.10", "acodec": "none", "dynamic_range": "HDR10", "filesize": 2248000000},
            {"format_id": "315", "ext": "webm", "height": 2160, "fps": 60, "vcodec": "vp9", "acodec": "none", "dynamic_range": "SDR", "filesize": 1870000000},
            {"format_id": "299", "ext": "mp4", "height": 1080, "fps": 60, "vcodec": "avc1.64002A", "acodec": "none", "dynamic_range": "SDR", "filesize": 412000000},
            {"format_id": "136", "ext": "mp4", "height": 720, "fps": 30, "vcodec": "avc1.4D401F", "acodec": "none", "dynamic_range": "SDR", "filesize_approx": 201000000}],
            "subtitles": {"de": [{"name": "German"}], "pt-PT": [{"name": "Portuguese (Portugal)"}],
                "ar": [{"name": "Arabic"}]},
            "automatic_captions": {"en-orig": [{"name": "English (Original)"}], "en": [{"name": "English"}]}}"#;
        if let Some(v) = mdm_tools::resolve::parse(SAMPLE.as_bytes()) {
            self.page = "https://video.example/talk".into();
            self.looked_up = self.page.clone();
            self.set_video(v);
        }
    }

    /// A page like YouTube's when yt-dlp has no JavaScript runtime (#351): videos and sound
    /// apart, nothing carrying both (`/show video-adaptive`); `runtime_said`: yt-dlp said why
    /// (#852, `/show video-adaptive:runtime`).
    pub fn show_sample_adaptive(&mut self, runtime_said: bool) {
        const SAMPLE: &str = r#"{"title": "A conference talk about download managers, recorded in Lisbon",
            "duration": 1830, "language": "en", "formats": [
            {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 29000000},
            {"format_id": "137", "ext": "mp4", "height": 1080, "vcodec": "avc1", "acodec": "none", "filesize": 412000000},
            {"format_id": "136", "ext": "mp4", "height": 720, "vcodec": "avc1", "acodec": "none", "filesize": 201000000}]}"#;
        if let Some(mut v) = mdm_tools::resolve::parse(SAMPLE.as_bytes()) {
            v.needs_js_runtime = runtime_said;
            self.page = "https://video.example/talk".into();
            self.looked_up = self.page.clone();
            self.set_video(v);
        }
    }

    /// A listed playlist, as [`crate::playlistdlg::PlaylistView::sample`] makes it, its quality
    /// the one at position `pick` (`/show video-playlist:7`).
    pub fn show_sample_list(&mut self, pick: &str) {
        self.page = "https://video.example/playlist".into();
        self.looked_up = self.page.clone();
        self.list = Some(crate::playlistdlg::PlaylistView::sample(pick));
    }

    /// The sample's choice whose label starts with `prefix` (any case), chosen and scrolled into
    /// view (`/show video-subs:flac`: a screenshot of a choice below the list's first rows;
    /// `video-subs:most`, the most compatible one, #1439).
    pub fn pick(&mut self, prefix: &str) {
        if !self.advanced {
            self.advanced = true;
            if let Some((video, _)) = self.video.take() {
                self.set_video(video);
            }
        }
        let prefix = prefix.to_lowercase();
        let found = self.video.as_ref().and_then(|(_, choices)| {
            choices.iter().position(|c| {
                let label = c.label.to_lowercase();
                label.starts_with(&prefix)
                    || label
                        .strip_prefix("audio only ")
                        .is_some_and(|l| l.starts_with(&prefix))
            })
        });
        if let Some(i) = found {
            self.chosen = i;
            // The list shows 8 rows.
            self.rows.show_row(i, 8);
            self.name_from_choice();
        }
    }

    /// Helpers..., as its button does (`/show video-helpers`).
    pub fn open_helpers(&mut self) {
        if self.helpers.is_none() {
            self.helpers = Some(crate::helpers::Programs::paths());
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, rt: &tokio::runtime::Handle) -> Outcome<VideoOut> {
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        if self.tool_check.elapsed() >= std::time::Duration::from_secs(2) {
            self.tool_check = std::time::Instant::now();
            let refreshed = tools();
            let had_ffmpeg = self.tools.as_ref().is_some_and(|tools| tools.2.is_some());
            self.tools = Some(refreshed);
            if had_ffmpeg != self.tools.as_ref().is_some_and(|tools| tools.2.is_some())
                && let Some((video, _)) = self.video.take()
            {
                self.set_video(video);
            }
        }
        if let Some(clip) = self.clip.take() {
            self.fill_from_clipboard(&clip);
        }
        // Results from the background work.
        let looked = self.lookup.as_ref().and_then(|s| s.lock().unwrap().take());
        if let Some(res) = looked {
            self.lookup = None;
            self.declared = None;
            match res {
                Ok((Looked::Video(v), declared)) => {
                    self.declared = declared;
                    self.set_video(v);
                }
                Ok((Looked::List(l), _)) => {
                    if let Some((exe, browser)) = self.looking.take() {
                        self.list = Some(crate::playlistdlg::PlaylistView::start(
                            &exe,
                            &self.looked_up,
                            browser,
                            l,
                        ));
                    }
                }
                Err(e) => {
                    let browser = self
                        .looking
                        .as_ref()
                        .map(|(_, b)| b)
                        .cloned()
                        .unwrap_or_default();
                    self.error = Some(request_error(&e, &browser));
                }
            }
        }
        let installed = self
            .install
            .as_ref()
            .and_then(|(t, s)| s.lock().unwrap().take().map(|r| (*t, r)));
        if let Some((tool, res)) = installed {
            self.install = None;
            self.tools = None;
            match res {
                Err(e) => {
                    let what = if tool == Tool::YtDlp {
                        "yt-dlp"
                    } else {
                        "ffmpeg"
                    };
                    self.error = Some(crate::i18n::fmt(
                        // i18n: an error: {what} = a helper program (yt-dlp, ffmpeg), {error} = why
                        crate::i18n::t("{what} could not be installed: {error}"),
                        &[("what", what), ("error", &e)],
                    ));
                }
                // With ffmpeg the qualities that need merging appear.
                Ok(_) if tool == Tool::Ffmpeg => {
                    if let Some((v, _)) = self.video.take() {
                        self.set_video(v);
                    }
                }
                Ok(_) => {}
            }
        }
        // A playlist's question before adding many (#69): Yes goes ahead in the window's frame.
        if let Some(list) = &mut self.list
            && let Some(now) = list.answer(ctx)
        {
            self.confirmed = Some(now);
            ctx.request_repaint();
        }
        let out = window(
            ctx,
            "video-page",
            "Download video from a page",
            [WIDTH, HEIGHT],
            false,
            winui::BG(),
            |ui, o| self.body(ui, o, rt),
        );
        // A playlist's quality chosen, a list or a video looked up: Save to follows (#611).
        self.follow_sound();
        // Helpers... open: on Close, the helpers are read again (another program, or MDM's own).
        if let Some(h) = &mut self.helpers
            && !matches!(h.show(ctx, rt), Outcome::Open)
        {
            self.helpers = None;
            self.tools = None;
            if let Some((v, _)) = self.video.take() {
                self.set_video(v);
            }
        }
        if self.use_config && matches!(out, Outcome::Done(())) {
            match self.preferences.profile(true) {
                Ok(profile) => {
                    return Outcome::Done(VideoOut::Configured {
                        page: self.page.trim().to_owned(),
                        context: self.context(),
                        profile,
                        start_now: self.start_now == Some(true),
                    });
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                    return Outcome::Open;
                }
            }
        }
        let folder = crate::dialogs::resolve_path(&self.folder);
        let out = crate::dialogs::ensure_folder(ctx, &mut self.asking, out, &folder);
        match out {
            Outcome::Open => Outcome::Open,
            Outcome::Closed => Outcome::Closed,
            Outcome::Done(()) => self.added().map_or(Outcome::Closed, Outcome::Done),
        }
    }

    pub fn take_saved_cookies(&mut self) -> Option<(String, String)> {
        self.helpers.as_mut()?.take_saved_cookies()
    }

    /// What Download Now or Later adds: a playlist's videos, each its own download, or the
    /// video looked up (none when there is nothing to add).
    fn added(&self) -> Option<VideoOut> {
        let folder = self.save_folder();
        let start_now = self.start_now.unwrap_or(true);
        if let Some(list) = &self.list {
            let merge = self.tools.as_ref().is_some_and(|t| t.2.is_some());
            return Some(VideoOut::Many(list.add(
                &folder,
                merge,
                start_now,
                &self.context(),
                self.category(),
            )));
        }
        let page = url::Url::parse(self.page.trim()).ok()?;
        let c = self.current_choice()?;
        let suggested = self.suggested_name(&c)?;
        let name = if self.name.trim().is_empty() {
            suggested.clone()
        } else {
            mdm_engine::naming::sanitize(self.name.trim())
        };
        let named = name != suggested;
        let audio = self.sound_only() == Some(true);
        let chosen = self.chosen_subs();
        // The video the page declares (#1076): its address, the page as its Referer.
        let declared = self
            .declared
            .as_ref()
            .filter(|_| self.looked_up == self.page.trim())
            .and_then(|d| Some((url::Url::parse(&d.address).ok()?, d)));
        let (page, context) = match declared {
            Some((address, d)) => (address, declared_context(&self.context(), d)),
            None => (page, self.context()),
        };
        Some(VideoOut::One(VideoAdd {
            ytdlp: self.manual_profile().ok().map(|profile| {
                if audio {
                    audio_profile(profile)
                } else {
                    profile
                }
            }),
            page,
            selector: c.selector.clone(),
            dest: folder.join(name),
            category: self.category(),
            size: c.size,
            start_now,
            context,
            subs: if audio {
                Subs::default()
            } else {
                subs_for_choice(&c, &chosen)
            },
            remember: (!self.subs.is_empty()).then_some(chosen),
            preloaded: self
                .kept
                .as_ref()
                .filter(|_| self.looked_up == self.page.trim())
                .map(|slot| mdm_tools::video::Preloaded {
                    slot: slot.clone(),
                    selector: c.selector.clone(),
                    label: c.label.clone(),
                    merge: self.tools.as_ref().is_some_and(|t| t.2.is_some()),
                }),
            named,
            duration: whole_seconds(self.video.as_ref().and_then(|v| v.0.duration)),
        }))
    }

    /// The choice (or the playlist's quality) saves only the sound; None while there is none.
    fn sound_only(&self) -> Option<bool> {
        if let Some(list) = &self.list {
            return Some(list.sound_only());
        }
        self.current_choice()
            .as_ref()
            .map(|choice| self.choice_is_audio(choice))
    }

    /// The category the download is listed under (#611), and whose folder "Change folder for
    /// this category on last selected" keeps: Music for a sound-only choice, else Video.
    pub fn category(&self) -> Category {
        if self.music_dir.is_some() && self.sound_only() == Some(true) {
            Category::Music
        } else {
            Category::Video
        }
    }

    /// Save to goes to the Music category's folder when the choice turns sound-only, back to
    /// Video's when it turns video again, but only while it holds the folder MDM put there: a
    /// folder the user typed or picked is theirs (#611).
    fn follow_sound(&mut self) {
        let Some(sound) = self.sound_only() else {
            return;
        };
        if sound == self.sound {
            return;
        }
        self.sound = sound;
        let Some(music) = &self.music_dir else {
            return;
        };
        let (from, to) = if sound {
            (&self.video_dir, music)
        } else {
            (music, &self.video_dir)
        };
        if crate::dialogs::resolve_path(&self.folder) == crate::dialogs::resolve_path(from) {
            self.folder = to.clone();
        }
    }

    /// The Save to folder: the one a playlist's own folder goes under, too.
    pub fn save_folder(&self) -> PathBuf {
        crate::dialogs::resolve_path(&self.folder)
    }

    fn current_choice(&self) -> Option<Choice> {
        let (video, choices) = self.video.as_ref()?;
        if let Some(code) = &self.format_code {
            return format_code_choice(video, code, &self.preferences.container).or_else(|| {
                choices
                    .iter()
                    .filter(|choice| mdm_tools::resolve::is_audio(choice))
                    .find(|choice| choice.selector == code.trim())
                    .cloned()
            });
        }
        choices.get(self.chosen).cloned()
    }

    fn choice_is_audio(&self, choice: &Choice) -> bool {
        let raw = self
            .format_code
            .as_deref()
            .and_then(|code| format_by_id(&self.video.as_ref()?.0, code.trim()));
        raw.map_or_else(
            || mdm_tools::resolve::is_audio(choice),
            |f| f.audio && !f.video,
        )
    }

    fn suggested_name(&self, choice: &Choice) -> Option<String> {
        let video = &self.video.as_ref()?.0;
        Some(mdm_tools::preferences::filename(
            &self.preferences.filename_template,
            &video.title,
            &video.naming,
            if self.choice_is_audio(choice) {
                &choice.ext
            } else {
                &self.preferences.container
            },
        ))
    }

    /// The default file name and category folder follow the choice.
    fn name_from_choice(&mut self) {
        if !self.name_edited
            && let Some(choice) = self.current_choice()
            && let Some(name) = self.suggested_name(&choice)
        {
            self.name = name;
        }
        self.follow_sound();
    }

    /// Install a helper in the background.
    fn install(
        &mut self,
        tool: Tool,
        dir: PathBuf,
        rt: &tokio::runtime::Handle,
        ctx: &egui::Context,
    ) {
        let slot: Slot<mdm_tools::Installed> = Arc::new(Mutex::new(None));
        let (out, ctx) = (slot.clone(), ctx.clone());
        let net = crate::settings::own_net();
        rt.spawn(async move {
            let (res, name) = match tool {
                Tool::YtDlp => (mdm_tools::install_latest(&dir, &net, None).await, "yt-dlp"),
                Tool::Ffmpeg => (
                    mdm_tools::ffmpeg::install_latest(&dir, &net, None).await,
                    "ffmpeg",
                ),
            };
            // What failed and what to do (#816): wrapped in the empty list below.
            *out.lock().unwrap() = Some(res.map_err(|e| crate::errors::tools_and_advice(&e, name)));
            ctx.request_repaint();
        });
        self.install = Some((tool, slot));
        self.error = None;
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2, rt: &tokio::runtime::Handle) -> Outcome<()> {
        let p = ui.painter().clone();
        let (dir, ytdlp, ffmpeg) = self.tools.get_or_insert_with(tools).clone();
        let busy = self.lookup.is_some() || self.install.is_some();
        let mut matched =
            (self.video.is_some() || self.list.is_some()) && self.page.trim() == self.looked_up;
        video_card(&p, o, 12.0, 77.0, None, true);
        video_label(ui, o, 39.0, "Page address", true);
        let (look_x, look_w) =
            winui::buttons_to_right(&p, RIGHT, 10.0, &[("Look up", 84.0)], 145.0)[0];
        let edit = winui::edit(
            ui,
            r(o, FIELD_X, 22.0, look_x - FIELD_X - 10.0, 27.0),
            &mut self.page,
            !busy,
            false,
        );
        if !self.focused {
            edit.request_focus();
            self.focused = true;
        }
        if edit.changed() {
            self.error = None;
            matched =
                (self.video.is_some() || self.list.is_some()) && self.page.trim() == self.looked_up;
        }
        let was_config = self.use_config;
        let config_label = "Use yt-dlp configuration file";
        let path_x = winui::checkbox_caption_end(&p, o.x + FIELD_X, config_label)
            .min(o.x + RIGHT - 110.0)
            + 8.0;
        winui::checkbox_until(
            ui,
            pos2(o.x + FIELD_X, o.y + 61.0),
            &mut self.use_config,
            config_label,
            !busy,
            Some(path_x - 4.0),
        );
        if was_config != self.use_config {
            self.error = None;
        }
        let config_path = mdm_tools::config::path();
        let config_status = config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| crate::i18n::t("No configuration file selected").into());
        winui::fit(
            ui,
            &p,
            path_x,
            o.y + 72.0,
            &config_status,
            winui::MUTED(),
            o.x + RIGHT,
            "video-config-path",
        );

        let page_ok = url::Url::parse(self.page.trim())
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some());
        let can_look = page_ok && !busy && !self.use_config && ytdlp.is_some();
        let look = winui::button(
            ui,
            r(o, look_x, 22.0, look_w, 27.0),
            "Look up",
            if !can_look {
                ButtonKind::Disabled
            } else if matched {
                ButtonKind::Normal
            } else {
                ButtonKind::Default
            },
            false,
            None,
        )
        .clicked()
            || (edit.has_focus() && can_look && winui::take_key(ui, egui::Key::Enter));
        if look
            && can_look
            && let Some(tool) = &ytdlp
        {
            matched = false;
            // A typed or pasted address too (#359); the box shows what is looked up.
            self.page = mdm_tools::pages::video_address(self.page.trim());
            let slot: Slot<(Looked, Option<Declared>)> = Arc::new(Mutex::new(None));
            let (out, exe, page, ctx, rt) = (
                slot.clone(),
                tool.exe.clone(),
                self.page.trim().to_owned(),
                ui.ctx().clone(),
                rt.clone(),
            );
            let browser = from_browser(&self.context());
            self.looking = Some((exe.clone(), browser.clone()));
            let kept = mdm_tools::video::InfoSlot::new(std::time::Instant::now());
            self.kept = Some(kept.clone());
            let profile = match self.manual_profile() {
                Ok(p) => p,
                Err(e) => {
                    self.error = Some(e);
                    return Outcome::Open;
                }
            };
            std::thread::spawn(move || {
                mdm_tools::config::with_profile(profile, || {
                    let res = look_up_cached(&exe, &page, &browser, &rt).map(
                        |(looked, declared, info)| {
                            kept.fill(info.json, info.without_cookies);
                            (looked, declared)
                        },
                    );
                    if res.is_err() {
                        kept.fail();
                    }
                    *out.lock().unwrap() = Some(res);
                    ctx.request_repaint();
                })
            });
            self.lookup = Some(slot);
            self.looked_up = self.page.trim().to_owned();
            self.video = None;
            self.format_code = None;
            self.advanced = false;
            self.name.clear();
            self.name_edited = false;
            // A list being listed stops (dropped).
            self.list = None;
            self.error = None;
        }

        let options = matched && !self.use_config && self.lookup.is_none();
        let installing = self.install.as_ref().map(|(tool, _)| *tool);
        let offer = if self.use_config {
            None
        } else if ytdlp.is_none() && mdm_tools::ytdlp_asset().is_some() {
            Some((Tool::YtDlp, "Install yt-dlp"))
        } else if ffmpeg.is_none() && mdm_tools::ffmpeg::available() {
            Some((Tool::Ffmpeg, "Install ffmpeg"))
        } else {
            None
        };
        let (status, error) = if let Some(error) = &self.error {
            (Some(error.as_str()), true)
        } else if let Some(installing) = installing {
            (
                Some(crate::i18n::t(if installing == Tool::YtDlp {
                    "Downloading and verifying yt-dlp..."
                } else {
                    "Downloading and verifying ffmpeg (about 80 MB)..."
                })),
                false,
            )
        } else if self.lookup.is_some() {
            (Some(crate::i18n::t("Looking the page up...")), false)
        } else if self.use_config {
            (
                Some(crate::i18n::t(if config_path.is_some() {
                    "Format, quality, save and embedding settings come from your yt-dlp configuration file."
                } else {
                    "Select a configuration file in yt-dlp settings."
                })),
                false,
            )
        } else if ytdlp.is_none() {
            (
                Some(crate::i18n::t(if offer.is_some() {
                    "Video lookups use yt-dlp (about 23 MB, checked against its signature)."
                } else {
                    "Choose a yt-dlp executable in yt-dlp settings."
                })),
                false,
            )
        } else if self
            .video
            .as_ref()
            .is_some_and(|(v, _)| options && v.needs_js_runtime)
        {
            (Some(js_runtime_note()), false)
        } else if ffmpeg.is_none() {
            (
                Some(crate::i18n::t(
                    "Install ffmpeg to merge or remux the selected formats.",
                )),
                false,
            )
        } else if !options {
            (
                Some(crate::i18n::t(
                    "Click Look up to load the qualities for this page.",
                )),
                false,
            )
        } else {
            (None, false)
        };
        let mut top = 99.0;
        if let Some(status) = status {
            let install = offer.filter(|_| !busy).map(|(tool, label)| {
                let w = winui::button_fit_width(&p, label, 105.0, 155.0);
                (tool, label, w)
            });
            let left = LEFT + 12.0;
            let right = RIGHT - install.map_or(0.0, |(_, _, w)| w + 12.0);
            let lines = winui::wrap_lines(&p, status, right - left);
            let height = (18.0 + 15.0 * lines.len().min(3) as f32).max(39.0);
            video_card(&p, o, top, height, None, true);
            let color = if error {
                winui::ERROR()
            } else {
                winui::accent()
            };
            p.rect_filled(r(o, LEFT, top + 10.0, 3.0, height - 20.0), 1.0, color);
            for (i, line) in lines.iter().take(3).enumerate() {
                text(
                    &p,
                    o.x + left,
                    o.y + top + 19.0 + i as f32 * 15.0,
                    line,
                    if error { color } else { winui::TEXT() },
                );
            }
            let status_rect = r(o, left, top, right - left, height);
            winui::readable(ui, status_rect, "video-status", status);
            if lines.len() > 3 {
                winui::full_on_hover(ui, status_rect, "video-status-full", status);
            }
            if let Some((tool, label, w)) = install
                && winui::button(
                    ui,
                    r(o, RIGHT - w, top + (height - 25.0) / 2.0, w, 25.0),
                    label,
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked()
                && let Some(dir) = dir
            {
                self.install(tool, dir, rt, ui.ctx());
            }
            top += height + 10.0;
        }

        self.format_card(ui, o + egui::vec2(0.0, top), options);
        top += 113.0;
        top += self.quality_card(ui, o + egui::vec2(0.0, top), options) + 10.0;
        self.save_card(ui, o + egui::vec2(0.0, top), options, ffmpeg.is_some());
        top += 108.0;
        let embedding = o + egui::vec2(0.0, top);
        video_card(&p, embedding, 0.0, 93.0, Some("Embedding"), options);
        let controls = embedding + egui::vec2(0.0, 31.0);
        let subtitles =
            self.list.is_none() && self.subtitles_row(ui, controls, options, ffmpeg.is_some());
        video_label(ui, controls, 21.0, "Subtitle languages", subtitles);
        self.chapters_row(ui, controls, options, ffmpeg.is_some());
        winui::checkbox_until(
            ui,
            pos2(o.x + FIELD_X + 158.0, controls.y + 43.0),
            &mut self.thumbnail,
            "Embed thumbnail when available",
            options && ffmpeg.is_some() && self.preferences.container != "webm",
            Some(o.x + RIGHT),
        );

        let listing = self.list.as_ref().is_some_and(|list| list.listing());
        let chosen = self.list.as_ref().map_or_else(
            || self.current_choice().is_some(),
            |list| list.chosen() > 0 && !listing,
        );
        let ready = !busy
            && page_ok
            && if self.use_config {
                config_path.is_some()
            } else {
                options && chosen && ffmpeg.is_some()
            };
        self.footer(ui, o, ready, listing)
    }

    fn format_card(&mut self, ui: &mut Ui, o: Pos2, enabled: bool) {
        let p = ui.painter().clone();
        video_card(&p, o, 0.0, 103.0, Some("Format"), enabled);
        video_label(ui, o, 47.0, "Output container", enabled);
        video_label(ui, o, 82.0, "Preferred codec", enabled && !self.advanced);
        let containers = ["Matroska (MKV)", "MPEG-4 (MP4)", "WebM"];
        let ci = mdm_tools::preferences::CONTAINERS
            .iter()
            .position(|c| *c == self.preferences.container)
            .unwrap_or(0);
        let mut changed = false;
        if let Some(i) = winui::combo_light(
            ui,
            r(o, FIELD_X, 30.0, 230.0, 26.0),
            &containers,
            ci,
            enabled,
            "video-container",
        ) {
            self.preferences.container = mdm_tools::preferences::CONTAINERS[i].into();
            if i == 2 && self.preferences.codec == 1 {
                self.preferences.codec = 2;
            }
            changed = true;
        }
        let ids: Vec<_> = (0..mdm_tools::preferences::CODECS.len())
            .filter(|i| self.preferences.container != "webm" || *i != 1)
            .collect();
        let codecs: Vec<_> = ids
            .iter()
            .map(|i| mdm_tools::preferences::CODECS[*i])
            .collect();
        let selected = ids
            .iter()
            .position(|i| *i == self.preferences.codec)
            .unwrap_or(0);
        if let Some(i) = winui::combo_light(
            ui,
            r(o, FIELD_X, 65.0, 230.0, 26.0),
            &codecs,
            selected,
            enabled && !self.advanced,
            "video-codec",
        ) {
            self.preferences.codec = ids[i];
            changed = true;
        }
        if self.format_code.is_some() {
            text(
                &p,
                o.x + FIELD_X + 242.0,
                o.y + 82.0,
                crate::i18n::t("Set by the format IDs"),
                winui::MUTED(),
            );
        }
        if changed {
            if !self.advanced
                && let Some((video, _)) = self.video.take()
            {
                let choices = self.preset_choices(Some(&video));
                self.video = Some((video, choices));
            }
            self.name_from_choice();
        }
    }

    fn quality_card(&mut self, ui: &mut Ui, o: Pos2, enabled: bool) -> f32 {
        let p = ui.painter().clone();
        if let Some(list) = &mut self.list {
            video_card(&p, o, 0.0, 236.0, Some("Quality"), enabled);
            ui.add_enabled_ui(enabled, |ui| {
                list.middle(ui, o + egui::vec2(LEFT - 11.0, -35.0), Some(489.0));
            });
            return 236.0;
        }

        let height = if self.format_code.is_some() {
            282.0
        } else {
            236.0
        };
        video_card(&p, o, 0.0, height, Some("Quality"), enabled);
        let shown = "Show all formats";
        let x = (RIGHT - winui::text_width(&p, crate::i18n::t(shown)) - 23.0).max(FIELD_X);
        let before = self.advanced;
        winui::checkbox_until(
            ui,
            pos2(o.x + x, o.y + 8.0),
            &mut self.advanced,
            shown,
            enabled,
            Some(o.x + RIGHT),
        );
        if before != self.advanced {
            if self.advanced {
                self.format_code = Some(String::new());
                if let Some((video, _)) = self.video.take() {
                    let merge = self.tools.as_ref().is_some_and(|t| t.2.is_some());
                    let choices = mdm_tools::resolve::choices(&video, merge);
                    self.video = Some((video, choices));
                }
            } else {
                self.format_code = None;
                if let Some((video, _)) = self.video.take() {
                    self.set_video(video);
                }
            }
            self.rows = Default::default();
        }
        if let Some(mut code) = self.format_code.take() {
            let previous = code.clone();
            self.format_table(ui, o, enabled, &mut code);
            let changed = code != previous;
            self.format_code = Some(code);
            if changed {
                self.name_from_choice();
            }
        } else {
            self.quality_list(ui, o, enabled);
        }
        if self.format_code.is_some() {
            282.0
        } else {
            236.0
        }
    }

    fn quality_list(&mut self, ui: &mut Ui, o: Pos2, enabled: bool) {
        let defaults = self.preset_choices(None);
        let choices = if enabled {
            self.video
                .as_ref()
                .map_or(defaults.as_slice(), |(_, c)| c.as_slice())
        } else {
            defaults.as_slice()
        };
        self.rows.selected = enabled
            .then_some(self.chosen)
            .filter(|i| *i < choices.len());
        self.rows.name = crate::i18n::t("Quality");
        let rect = r(o, LEFT, 30.0, RIGHT - LEFT, 193.0);
        let picked = ui
            .add_enabled_ui(enabled, |ui| {
                let ev = winui::row_list(
                    ui,
                    "video-choices",
                    rect,
                    choices.len(),
                    21.0,
                    &mut self.rows,
                    |ui, clip, i, row, look| {
                        let c = &choices[i];
                        let label = choice_label(&c.label);
                        let (size, said) = choice_size(c);
                        if enabled {
                            let file = format!("video.{}", c.ext);
                            crate::fileicons::paint_at(
                                ui,
                                clip,
                                crate::fileicons::Kind::File(&file),
                                pos2(row.left() + 8.0, row.top() + 2.0),
                                16.0,
                            );
                        }
                        let ink = if enabled {
                            look.ink
                        } else {
                            winui::TEXT_DISABLED()
                        };
                        text(clip, row.left() + 30.0, row.top() + 14.0, &label, ink);
                        winui::text_right(clip, row.right() - 9.0, row.top() + 14.0, &size, ink);
                        look.name = format!("{label}, {said}");
                    },
                );
                if ev.focused
                    && self.rows.type_to(ui, "video-choices", choices.len(), |i| {
                        choices[i].label.clone()
                    })
                {
                    self.rows.selected
                } else {
                    ev.picked
                }
            })
            .inner;
        if enabled && let Some(i) = picked {
            self.chosen = i;
            if !self.advanced {
                self.preferences.quality = i;
            }
            self.name_from_choice();
        }
    }

    fn format_table(&mut self, ui: &mut Ui, o: Pos2, enabled: bool, code: &mut String) {
        let Some((video, choices)) = &self.video else {
            return;
        };
        let p = ui.painter().clone();
        let area = r(o, LEFT, 29.0, RIGHT - LEFT, 170.0);
        let header = [
            "ID",
            "EXT",
            "RESOLUTION",
            "FPS",
            "FILESIZE",
            "TBR",
            "VCODEC",
            "ACODEC",
            "MORE INFO",
        ];
        format_columns(&p, o.x + LEFT, area.top() + 12.0, header, winui::MUTED());
        self.rows.name = crate::i18n::t("Available formats");
        let picked = ui
            .add_enabled_ui(enabled, |ui| {
                winui::row_list(
                    ui,
                    "video-formats",
                    r(o, LEFT, 47.0, RIGHT - LEFT, 150.0),
                    video.formats.len(),
                    19.0,
                    &mut self.rows,
                    |_, clip, i, row, look| {
                        let f = &video.formats[i];
                        let cells = format_cells(f);
                        let selected = code.split('+').any(|id| id.trim() == f.id);
                        if selected {
                            clip.rect_filled(r(row.min, 1.0, 3.0, 3.0, 13.0), 1.0, winui::accent());
                        }
                        format_columns(
                            clip,
                            row.left(),
                            row.top() + 13.0,
                            cells.each_ref().map(String::as_str),
                            look.ink,
                        );
                        look.name = cells.join(", ");
                    },
                )
                .picked
            })
            .inner;
        if enabled && let Some(i) = picked {
            pick_format(video, code, i);
        }
        video_label(ui, o, 227.0, "Format code", enabled);
        winui::edit(
            ui,
            r(o, FIELD_X, 210.0, RIGHT - FIELD_X - 215.0, 26.0),
            code,
            enabled,
            false,
        );
        let presets: Vec<_> = choices
            .iter()
            .filter(|choice| mdm_tools::resolve::is_audio(choice))
            .collect();
        let mut labels = vec!["Audio presets…".to_owned()];
        labels.extend(presets.iter().map(|c| choice_label(&c.label)));
        let names: Vec<_> = labels.iter().map(String::as_str).collect();
        let selected = presets.iter().position(|c| c.selector == code.trim());
        if let Some(i) = winui::combo_light(
            ui,
            r(o, RIGHT - 205.0, 210.0, 205.0, 26.0),
            &names,
            selected.map_or(0, |i| i + 1),
            enabled && !presets.is_empty(),
            "video-audio-presets",
        ) {
            *code = i
                .checked_sub(1)
                .map_or_else(String::new, |i| presets[i].selector.clone());
        }
        let invalid = !code.trim().is_empty()
            && !presets.iter().any(|c| c.selector == code.trim())
            && format_code_choice(video, code, &self.preferences.container).is_none();
        let (hint, color) = if invalid {
            (
                "Choose a media ID, or combine a video-only and audio-only ID with +.",
                winui::ERROR(),
            )
        } else {
            (
                "Choose one format ID, or select video and audio to join their IDs with +.",
                winui::MUTED(),
            )
        };
        winui::paragraph(
            &p,
            o.x + FIELD_X,
            o.y + 254.0,
            crate::i18n::t(hint),
            color,
            o.x + RIGHT,
            14.0,
        );
    }

    fn save_card(&mut self, ui: &mut Ui, o: Pos2, enabled: bool, ffmpeg: bool) {
        let p = ui.painter().clone();
        video_card(&p, o, 0.0, 98.0, Some("Save"), enabled);
        video_label(ui, o, 47.0, "Save to", enabled);
        winui::edit(
            ui,
            r(o, FIELD_X, 30.0, RIGHT - FIELD_X - 48.0, 26.0),
            &mut self.folder,
            enabled,
            false,
        );
        if winui::button(
            ui,
            r(o, RIGHT - 38.0, 30.0, 38.0, 26.0),
            "...",
            if enabled {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
            None,
        )
        .clicked()
        {
            let from = self.folder.clone();
            winui::start_pick(ui.ctx(), "video-folder", move || {
                rfd::FileDialog::new().set_directory(from).pick_folder()
            });
        }
        if let Some(folder) = winui::picked(ui.ctx(), "video-folder") {
            self.folder = folder.display().to_string();
        }
        let field = r(o, FIELD_X, 64.0, RIGHT - FIELD_X, 26.0);
        if let Some(list) = &mut self.list {
            video_label(ui, o, 81.0, "Quality", enabled);
            ui.add_enabled_ui(enabled, |ui| list.quality_row(ui, field, ffmpeg));
        } else {
            video_label(ui, o, 81.0, "File name", enabled);
            let edit = winui::edit(ui, field, &mut self.name, enabled, false);
            self.name_edited |= edit.changed();
            if self.name.is_empty() && !edit.has_focus() {
                text(
                    &p,
                    o.x + FIELD_X + 5.0,
                    o.y + 81.0,
                    crate::i18n::t("Uses the video title if left empty"),
                    winui::MUTED(),
                );
            }
        }
    }

    fn footer(&mut self, ui: &mut Ui, o: Pos2, ready: bool, listing: bool) -> Outcome<()> {
        let p = ui.painter().clone();
        let y = HEIGHT - 40.0;
        winui::etched_h(&p, o.x + 24.0, o.y + y - 10.0, WIDTH - 48.0);
        let [now, later, cancel] = winui::buttons_to_right(
            &p,
            WIDTH - 24.0,
            8.0,
            &[("Download Now", 128.0), ("Later", 84.0), ("Cancel", 84.0)],
            165.0,
        )[..] else {
            unreachable!("three buttons")
        };
        let helpers = winui::button_fit_width(&p, "yt-dlp...", 84.0, 175.0);
        if winui::button(
            ui,
            r(o, 24.0, y, helpers, 28.0),
            "yt-dlp...",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
        {
            self.open_helpers();
        }
        let button = |ui: &mut Ui, (x, w), label, action, enabled| {
            let kind = match (enabled, action) {
                (false, _) => ButtonKind::Disabled,
                (true, true) => ButtonKind::Default,
                (true, false) => ButtonKind::Normal,
            };
            winui::button(ui, r(o, x, y, w, 28.0), label, kind, false, None)
        };
        let now = winui::why_disabled(
            button(ui, now, "Download Now", true, ready),
            listing,
            "Wait for the list, or stop listing",
        )
        .clicked();
        let later = button(ui, later, "Later", false, ready).clicked();
        let cancel = button(ui, cancel, "Cancel", false, true).clicked();
        let asking = self.list.as_ref().is_some_and(|l| l.asking());
        if cancel || (winui::escape_pressed(ui) && !asking) {
            return Outcome::Closed;
        }
        if ready && (now || later) {
            if !self.use_config
                && let Some(list) = &mut self.list
                && list.ask(now)
            {
                return Outcome::Open;
            }
            self.start_now = Some(now);
            return Outcome::Done(());
        }
        if let Some(now) = self.confirmed.take()
            && ready
            && !self.use_config
        {
            self.start_now = Some(now);
            return Outcome::Done(());
        }
        Outcome::Open
    }
}

impl VideoDialog {
    /// Embed chapters (#827), under Subtitles, for a video and a playlist alike: the chapters a
    /// video's page lists go into its file (ffmpeg writes them; on by default). Greyed without
    /// ffmpeg, and for a choice converted to MP3, Opus or FLAC; its tick stays as it was.
    fn chapters_row(&mut self, ui: &mut Ui, o: Pos2, enabled: bool, ffmpeg: bool) {
        let converts = match &self.list {
            Some(list) => list.converts(),
            None => self
                .current_choice()
                .is_some_and(|c| mdm_tools::resolve::audio_conversion(&c.selector).is_some()),
        };
        let can = enabled && ffmpeg && !converts;
        let mut on = self.chapters;
        let resp = winui::checkbox_until(
            ui,
            pos2(o.x + FIELD_X, o.y + 43.0),
            &mut on,
            // i18n: checkbox in Download video: the chapters a video's page lists (YouTube's, for one) go into the video file, where players show them and skip between them
            "Embed chapters",
            can,
            Some(o.x + FIELD_X + 153.0),
        );
        let resp = winui::why_disabled(
            resp,
            !ffmpeg,
            // i18n: why "Embed chapters" is greyed: ffmpeg (a helper program) is not installed
            "This needs ffmpeg",
        );
        winui::why_disabled(
            resp,
            ffmpeg && converts,
            // i18n: why "Embed chapters" is greyed in Download video: the choice is a sound converted to MP3, Opus or FLAC, which MDM saves without chapters
            "MP3, Opus and FLAC files are saved without chapters",
        );
        if can {
            self.chapters = on;
        }
    }

    /// Embed chapters' tick, for the settings (`video_chapters`).
    pub fn chapters(&self) -> bool {
        self.chapters
    }

    /// Subtitles (#68): a language the video has, from the uploader or the site's automatic
    /// captions, saved beside the video (`<name>.<language>.srt`) or put into it (ffmpeg).
    /// The list starts at `bx`, after the labels' column; true when it can be chosen from (its
    /// label is greyed otherwise).
    fn subtitles_row(&mut self, ui: &mut Ui, o: Pos2, enabled: bool, ffmpeg: bool) -> bool {
        let choice = self.current_choice();
        let audio = choice
            .as_ref()
            .is_some_and(|choice| self.choice_is_audio(choice));
        let on = enabled && choice.is_some() && !self.subs.is_empty() && !audio;
        let mut items = sub_items(&self.subs, self.video.is_some());
        items.insert(0, "Preferred subtitle languages".into());
        let shown: Vec<&str> = items.iter().map(String::as_str).collect();
        let pick = if self.preferred_subtitles {
            0
        } else {
            self.sub_pick + 1
        };
        if let Some(i) = winui::combo_light(
            ui,
            r(o, FIELD_X, 4.0, 230.0, 25.0),
            &shown,
            if enabled {
                pick.min(shown.len().saturating_sub(1))
            } else {
                1
            },
            on,
            "video-subs",
        ) {
            self.preferred_subtitles = i == 0;
            if i > 0 {
                self.sub_pick = i - 1;
            }
        }
        let chosen = on && (self.preferred_subtitles || self.sub_pick > 0);
        let holds = choice
            .as_ref()
            .is_some_and(mdm_tools::resolve::holds_subtitles);
        let mut embed = self.embed && (chosen || !enabled);
        let resp = winui::checkbox_until(
            ui,
            pos2(o.x + FIELD_X + 242.0, o.y + 10.0),
            &mut embed,
            // i18n: checkbox in Download video: subtitles go inside the video file instead of a separate .srt file beside it
            "Embed subtitles",
            chosen && ffmpeg && holds,
            Some(o.x + RIGHT),
        );
        let resp = winui::why_disabled(
            resp,
            chosen && !ffmpeg,
            // i18n: why "Embed subtitles" is greyed: ffmpeg (a helper program) is not installed
            "This needs ffmpeg",
        );
        winui::why_disabled(
            resp,
            chosen && ffmpeg && !holds,
            // i18n: why "Embed subtitles" is greyed: the chosen file type can't hold subtitles
            "Only MP4 and MKV videos can hold subtitles",
        );
        if chosen && ffmpeg && holds {
            self.embed = embed;
        }
        on
    }
}

fn video_card(
    p: &egui::Painter,
    o: Pos2,
    top: f32,
    height: f32,
    title: Option<&str>,
    enabled: bool,
) {
    let colors = crate::theme::colors();
    let bounds = r(o, 24.0, top, WIDTH - 48.0, height);
    let surface = if enabled {
        colors.tab_page
    } else {
        colors.window
    };
    p.rect_filled(bounds, 5.0, surface);
    if let Some(title) = title {
        winui::text_font(
            p,
            winui::tahoma_bold(),
            o.x + LEFT,
            o.y + top + 20.0,
            crate::i18n::t(title),
            if enabled {
                colors.text
            } else {
                colors.text_disabled
            },
        );
    }
}

fn video_label(ui: &Ui, o: Pos2, y: f32, label: &str, enabled: bool) {
    winui::fit(
        ui,
        ui.painter(),
        o.x + LEFT,
        o.y + y,
        crate::i18n::t(label),
        if enabled {
            winui::TEXT()
        } else {
            winui::TEXT_DISABLED()
        },
        o.x + FIELD_X - 10.0,
        ("video-label", label),
    );
}

fn format_by_id<'a>(video: &'a Video, id: &str) -> Option<&'a mdm_tools::resolve::Format> {
    (!id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)))
    .then(|| {
        video
            .formats
            .iter()
            .find(|f| f.id == id && (f.video || f.audio))
    })
    .flatten()
}

fn format_code_choice(video: &Video, code: &str, container: &str) -> Option<Choice> {
    let mut ids = code.split('+').map(str::trim);
    let first = format_by_id(video, ids.next()?)?;
    let audio = if let Some(id) = ids.next() {
        let other = format_by_id(video, id)?;
        if !first.video || first.audio || other.video || !other.audio || ids.next().is_some() {
            return None;
        }
        Some(other)
    } else {
        None
    };
    let selector = audio.map_or_else(|| first.id.clone(), |a| format!("{}+{}", first.id, a.id));
    let size = match audio {
        Some(a) => first
            .size
            .zip(a.size)
            .map(|(video, audio)| video.saturating_add(audio)),
        None => first.size,
    };
    Some(Choice {
        label: selector.clone(),
        selector,
        ext: if first.video {
            container.into()
        } else {
            first.ext.clone()
        },
        size,
        approx: first.approx || audio.is_some_and(|a| a.approx),
        height: first.height,
        ..Default::default()
    })
}

fn pick_format(video: &Video, code: &mut String, index: usize) {
    let picked = &video.formats[index];
    if format_by_id(video, &picked.id).is_none() {
        return;
    }
    let mut current = code
        .split('+')
        .filter_map(|id| format_by_id(video, id.trim()));
    let matching = if picked.video && !picked.audio {
        current
            .find(|f| f.audio && !f.video)
            .map(|audio| format!("{}+{}", picked.id, audio.id))
    } else if picked.audio && !picked.video {
        current
            .find(|f| f.video && !f.audio)
            .map(|video| format!("{}+{}", video.id, picked.id))
    } else {
        None
    };
    *code = matching.unwrap_or_else(|| picked.id.clone());
}

fn format_cells(format: &mdm_tools::resolve::Format) -> [String; 9] {
    let resolution = match (format.width, format.height) {
        (Some(w), Some(h)) => format!("{w}x{h}"),
        (_, Some(h)) => format!("{h}p"),
        _ if format.audio && !format.video => crate::i18n::t("Audio only").into(),
        _ => String::new(),
    };
    let size = format.size.map_or_else(String::new, |size| {
        let approximate = if format.approx { "~" } else { "" };
        format!("{approximate}{}", crate::dialogs::dialog_size(size))
    });
    [
        format.id.clone(),
        format.ext.clone(),
        resolution,
        format.fps.map_or_else(String::new, |n| format!("{n:.0}")),
        size,
        format.tbr.map_or_else(String::new, |n| format!("{n:.0}k")),
        format.vcodec.clone().unwrap_or_default(),
        format.acodec.clone().unwrap_or_default(),
        format.note.clone().unwrap_or_default(),
    ]
}

fn format_columns(p: &egui::Painter, x: f32, baseline: f32, values: [&str; 9], ink: egui::Color32) {
    let mut x = x + 8.0;
    let font = egui::FontId::monospace(11.0);
    for (value, width) in values
        .into_iter()
        .zip([43.0, 48.0, 79.0, 31.0, 71.0, 48.0, 83.0, 83.0, 100.0])
    {
        let label = winui::ellipsize(p, &font, value, width - 5.0);
        winui::text_font(p, font.clone(), x, baseline, &label, ink);
        x += width;
    }
}

/// "All the uploader's" is offered when they gave two languages or more.
fn all_offered(subs: &[SubChoice]) -> bool {
    subs.iter().filter(|s| !s.automatic).count() >= 2
}

/// The Subtitles list (English where it is MDM's words; combo_light shows them translated):
/// None, All the uploader's, then each language, the automatic ones marked.
fn sub_items(subs: &[SubChoice], looked_up: bool) -> Vec<String> {
    let mut out = vec![if looked_up && subs.is_empty() {
        // i18n: in the Subtitles list of Download video: the video has no subtitles
        crate::i18n::mark("None on this page").to_owned()
    } else {
        // i18n: in the Subtitles list of Download video: download no subtitles
        crate::i18n::mark("None").to_owned()
    }];
    if all_offered(subs) {
        // i18n: in the Subtitles list of Download video: every language the video's uploader gave
        out.push(crate::i18n::mark("All the uploader's").to_owned());
    }
    for s in subs {
        out.push(if s.automatic {
            crate::i18n::fmt(
                // i18n: in the Subtitles list of Download video: {language} = a language's name; the site made these subtitles automatically (speech recognition)
                crate::i18n::t("{language}, automatic"),
                &[("language", &s.name)],
            )
        } else {
            s.name.clone()
        });
    }
    out
}

/// The entry of the Subtitles list that `pref` (the last video's choice) stands for in this
/// video's: the same language (the uploader's before the automatic one, then any variant of
/// it: `pt` for `pt-BR`), every uploader's language again, or none.
fn pick_for(subs: &[SubChoice], pref: &Subs) -> usize {
    let all = all_offered(subs);
    let first = 1 + usize::from(all);
    let Some(want) = pref.langs.first() else {
        return 0;
    };
    if want == ALL_UPLOADERS {
        return if all {
            1
        } else {
            subs.iter()
                .position(|s| !s.automatic)
                .map_or(0, |i| i + first)
        };
    }
    let base = |c: &str| c.split(['-', '_']).next().unwrap_or(c).to_ascii_lowercase();
    subs.iter()
        .position(|s| s.code == *want && s.automatic == pref.automatic)
        .or_else(|| subs.iter().position(|s| s.code == *want))
        .or_else(|| subs.iter().position(|s| base(&s.code) == base(want)))
        .map_or(0, |i| i + first)
}

/// What the Subtitles list's entry `pick` asks yt-dlp for (0: none, then "All the uploader's"
/// when offered, then `subs` in order), put into the video when `embed`.
fn subs_at(subs: &[SubChoice], pick: usize, embed: bool) -> Subs {
    let all = all_offered(subs);
    match pick {
        0 => Subs {
            embed,
            ..Default::default()
        },
        1 if all => Subs {
            langs: vec![ALL_UPLOADERS.into()],
            automatic: false,
            embed,
        },
        i => match subs.get(i - 1 - usize::from(all)) {
            Some(c) => Subs {
                langs: vec![c.code.clone()],
                automatic: c.automatic,
                embed,
            },
            None => Subs {
                embed,
                ..Default::default()
            },
        },
    }
}

/// The subtitles saved with choice `c`, from the Subtitles list's choice `chosen`: none for a
/// sound-only choice; into the video only where its file type holds them (and with ffmpeg,
/// which VideoJob checks).
pub(crate) fn subs_for_choice(c: &Choice, chosen: &Subs) -> Subs {
    if mdm_tools::resolve::is_audio(c) {
        Subs::default()
    } else {
        Subs {
            embed: chosen.embed && mdm_tools::resolve::holds_subtitles(c),
            ..chosen.clone()
        }
    }
}

/// The subtitles the window would save with choice `c` of video `v` as it opens, untouched:
/// the last video's choice (`pref`, settings.video_subs) where this video has that language
/// ([`pick_for`]), else none. The browser's quality menu adds its choice with these.
pub(crate) fn default_subs(v: &Video, c: &Choice, pref: &Subs) -> Subs {
    let subs = mdm_tools::resolve::sub_choices(v, crate::i18n::code());
    subs_for_choice(c, &subs_at(&subs, pick_for(&subs, pref), pref.embed))
}

/// #852: the help page's section on installing a JavaScript runtime for yt-dlp.
pub(crate) const JS_RUNTIME_HELP: &str = "video.md#javascript-runtime";

/// #852: what a YouTube look-up yt-dlp made without a JavaScript runtime says: in the Download
/// video window, and in the browser's quality menu.
pub(crate) fn js_runtime_note() -> &'static str {
    // i18n: in Download video and the browser's video menu: yt-dlp had no JavaScript runtime, so YouTube listed only some qualities (often only the sound); Node.js and Deno are program names
    crate::i18n::t(
        "YouTube needs a JavaScript runtime to list its video qualities: install Node.js or Deno, then look it up again",
    )
}

/// The title of the page the browser played video `v` on (`mdm_ipc::Command::video_title`,
/// trimmed), when yt-dlp knows `v` only by the address it looked up, `page` (on some servers
/// that part is a long hash, #345), or only by the page's own title (`Video::page_titled`: it has
/// no reader for the site, and a player embedded in another site's page has its own: "Watch
/// file.mp4 - VOE", where the tab names the episode, #1351). None otherwise: the title a site's
/// reader knows the video by stays (a YouTube video in an article's frame keeps its own). The
/// Download video window and the browser's quality menu both name a video by it.
///
/// A page that shows one file is often titled with the file's name (Dropbox's, a server's own
/// listing: `Episode 31.mp4`): its video or sound extension is left out, as the choice's type
/// follows the name (it was `Episode 31.mp4.mp4`, #622).
pub(crate) fn page_title<'a>(
    context: &'a mdm_ipc::Command,
    v: &Video,
    page: &str,
) -> Option<&'a str> {
    context
        .video_title
        .as_deref()
        .map(|t| without_media_extension(t.trim()))
        .filter(|t| {
            !t.is_empty()
                && (v.page_titled || mdm_tools::resolve::named_after_address(&v.title, page))
        })
}

/// `title` without a video or sound file's extension at its end (`Episode 31.mp4` → `Episode
/// 31`); as it is otherwise, or when nothing would be left.
fn without_media_extension(title: &str) -> &str {
    let media = |ext: &str| {
        mdm_engine::naming::TYPES.iter().any(|(t, e)| {
            e.eq_ignore_ascii_case(ext)
                && (t.starts_with("video/") || t.starts_with("audio/") || *t == "application/ogg")
        })
    };
    match title.rsplit_once('.') {
        Some((stem, ext)) if media(ext) && !stem.trim().is_empty() => stem.trim_end(),
        _ => title,
    }
}

/// The file name a choice of type `c` gets for a video titled `title`: made safe, with the
/// choice's type.
pub(crate) fn file_name(title: &str, c: &Choice) -> String {
    let stem = mdm_engine::naming::sanitize(title);
    format!("{stem}.{}", c.ext)
}

/// The Download video window's look-up of `page` with the browser's context: yt-dlp's, or, when
/// yt-dlp read the page generically, the video the page declares its own (#1076,
/// `mdm_tools::declared`, the page read as Options says MDM's own requests go), which the
/// download then keeps as its address. A plain file's size is asked of its server, as yt-dlp
/// doesn't say it (#320).
pub(crate) fn look_up(
    exe: &std::path::Path,
    page: &str,
    browser: &mdm_tools::proc::FromBrowser,
    rt: &tokio::runtime::Handle,
) -> Result<(Looked, Option<Declared>), String> {
    look_up_cached(exe, page, browser, rt).map(|(v, d, _)| (v, d))
}

pub(crate) fn look_up_cached(
    exe: &std::path::Path,
    page: &str,
    browser: &mdm_tools::proc::FromBrowser,
    rt: &tokio::runtime::Handle,
) -> Result<(Looked, Option<Declared>, mdm_tools::proc::LookedUp), String> {
    let reader = mdm_tools::streams::Engine {
        handle: rt.clone(),
        opts: crate::settings::own_net(),
    };
    let (mut looked, info, declared) =
        mdm_tools::declared::look_up(exe, page, browser, &reader).map_err(|e| e.to_string())?;
    if let Looked::Video(v) = &mut looked
        && v.needs_size()
    {
        let size = match &declared {
            Some(d) => direct_size(rt, &d.address, &d.browser(browser)),
            None => direct_size(rt, page, browser),
        };
        if let Some(size) = size {
            v.set_size(size);
        }
    }
    Ok((looked, declared, info))
}

/// The browser's request context `context` for the video a page declares (#1076, `d`), as its
/// download keeps it: the page as its Referer, the browser's User-Agent and cookie jar, not the
/// page's own Cookie header nor a stream's Origin (`Declared::browser`).
pub(crate) fn declared_context(context: &mdm_ipc::Command, d: &Declared) -> mdm_ipc::Command {
    mdm_ipc::Command {
        referer: Some(d.page.clone()),
        origin: None,
        cookies: None,
        ..context.clone()
    }
}

/// yt-dlp's look-up of `page` with the browser's context, as the browser's quality menu makes it
/// (videolookup.rs), with its answer as it gave it, when it gave one (kept for the download
/// chosen there, `mdm_tools::video::Preloaded`): a plain file's size is asked of its server, as
/// yt-dlp doesn't say it (#320).
pub(crate) fn look_up_kept(
    exe: &std::path::Path,
    page: &str,
    browser: &mdm_tools::proc::FromBrowser,
    rt: &tokio::runtime::Handle,
) -> (Result<Looked, String>, Option<mdm_tools::proc::LookedUp>) {
    let (mut res, kept) = match mdm_tools::resolve::look_up_kept(exe, page, browser) {
        Ok((looked, out)) => (Ok(looked), Some(out)),
        Err(e) => (Err(e.to_string()), None),
    };
    if let Ok(Looked::Video(v)) = &mut res
        && v.needs_size()
        && let Some(size) = direct_size(rt, page, browser)
    {
        v.set_size(size);
    }
    (res, kept)
}

/// yt-dlp's view of the browser's request context.
pub fn from_browser(c: &mdm_ipc::Command) -> mdm_tools::proc::FromBrowser {
    mdm_tools::proc::FromBrowser {
        private: c.private,
        referer: c.referer.clone(),
        origin: c.origin.clone(),
        user_agent: c.user_agent.clone(),
        cookies: c.cookies.clone(),
        jar: c.cookie_jar.clone(),
    }
}

/// A saved row keeps its private origin when its memory-only browser context is gone.
pub fn from_item(item: &crate::model::Item) -> mdm_tools::proc::FromBrowser {
    mdm_tools::proc::FromBrowser {
        private: item.is_private_window(),
        ..from_browser(&item.context)
    }
}

/// Explain an authentication refusal without suggesting the normal profile for a private row.
pub(crate) fn private_auth_error(e: &str) -> Option<String> {
    let lower = e.to_ascii_lowercase();
    [
        "sign in", "log in", "login", "private video", "members-only", "members only",
        "authentication", "anti-bot", "not a bot", "confirm your age",
    ]
    .iter()
    .any(|s| lower.contains(s))
    .then(|| {
        let hint = crate::i18n::t(
            // i18n: a private-window video's authentication failure: only cookies supplied by that private window may be used, even after restarting MDM
            "Private-window downloads do not use Cookies for video sites from Options. Open the page in a private window, sign in there, and use its Download video button again.",
        );
        format!("{e}\n\n{hint}")
    })
}

/// Whose cookies a look-up actually used, for its error message as well as its arguments.
pub(crate) fn request_error(e: &str, browser: &mdm_tools::proc::FromBrowser) -> String {
    if browser.private
        && let Some(message) = private_auth_error(e)
    {
        return message;
    }
    let sent = !browser.jar.is_empty()
        || browser
            .cookies
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty());
    let store = (!browser.private && !sent)
        .then(mdm_tools::proc::browser_cookies)
        .flatten();
    lookup_error(e, sent, store.as_ref())
}

/// The size the server says for a plain file at `page`, asked as the browser would (its Referer,
/// its Origin, its cookies, its user agent). None when it won't say: the window then says "size unknown", as
/// it always did. A cookie jar (a whole site's cookies, for yt-dlp) isn't used: the header is.
fn direct_size(
    rt: &tokio::runtime::Handle,
    page: &str,
    browser: &mdm_tools::proc::FromBrowser,
) -> Option<u64> {
    use reqwest::header::{COOKIE, HeaderMap, HeaderValue, ORIGIN, REFERER};
    let url = url::Url::parse(page).ok()?;
    let mut headers = HeaderMap::new();
    for (name, value) in [
        (REFERER, &browser.referer),
        (ORIGIN, &browser.origin),
        (COOKIE, &browser.cookies),
    ] {
        if let Some(v) = value.as_deref().and_then(|v| HeaderValue::from_str(v).ok()) {
            headers.insert(name, v);
        }
    }
    let mut opts = crate::settings::own_net();
    if let Some(agent) = &browser.user_agent {
        opts.speak_as(agent.clone());
    }
    rt.block_on(mdm_engine::probe(url, headers, &opts))
        .ok()?
        .size
}

/// A browser's name as people know it, from yt-dlp's (`chrome`: Chrome).
pub fn browser_label(name: &str) -> &'static str {
    match name {
        "firefox" => "Firefox",
        "chrome" => "Chrome",
        "edge" => "Edge",
        "brave" => "Brave",
        "opera" => "Opera",
        "vivaldi" => "Vivaldi",
        "chromium" => "Chromium",
        "whale" => "Whale",
        "safari" => "Safari",
        _ => "?",
    }
}

/// yt-dlp's reason in plain words where it is one users meet often; otherwise its own text.
/// `sent`: the browser's extension passed the page's cookies; `store`: else, the browser whose
/// cookies yt-dlp read (Options → Sites Logins → Cookies for video sites, #229).
pub(crate) fn lookup_error(
    e: &str,
    sent: bool,
    store: Option<&mdm_tools::proc::BrowserCookies>,
) -> String {
    let lower = e.to_ascii_lowercase();
    let browser = store.map_or("?", |b| browser_label(&b.browser));
    let named = |text: &str| crate::i18n::fmt(crate::i18n::t(text), &[("browser", browser)]);
    // The browser's cookies yt-dlp couldn't read come first: the site's "sign in" that follows
    // is only their result.
    if store.is_some() && mdm_tools::proc::cookie_trouble(&lower) {
        return if lower.contains("could not copy chrome cookie database") {
            // i18n: the Download video window: yt-dlp couldn't read the cookies of the browser chosen in Options (Chrome, Edge...), which holds them while it runs; {browser} = its name
            named(
                "yt-dlp can't read {browser}'s cookies while {browser} is open. Close {browser} and look the page up again, or choose Firefox in yt-dlp settings.",
            )
        } else if lower.contains("could not find") {
            // i18n: the Download video window: the browser chosen in Options has no cookies on this computer (not installed, or another profile); {browser} = its name
            named(
                "yt-dlp found no cookies of {browser} on this computer. Check the browser and its profile in yt-dlp settings.",
            )
        } else if cfg!(windows)
            && (lower.contains("decrypt") || lower.contains("unknown cookie version"))
        {
            // i18n: the Download video window: Chrome 127 and later (and Edge, Brave) on Windows encrypt their cookies so only they can read them; {browser} = its name
            named(
                "{browser} keeps its cookies locked to itself on Windows, so yt-dlp can't read them. Choose Firefox in yt-dlp settings, or open the video with the Orbit browser extension.",
            )
        } else if ["keyring", "secretstorage", "kwallet", "decrypt"]
            .iter()
            .any(|k| lower.contains(k))
        {
            // i18n: the Download video window (Linux, macOS): the key to the browser's cookies is in the system's keyring (GNOME Keyring, KWallet, the macOS keychain), which didn't give it; {browser} = its name
            named(
                "yt-dlp couldn't get the key to {browser}'s cookies from the system's keyring. Unlock the keyring and look the page up again.",
            )
        } else {
            crate::i18n::fmt(
                // i18n: the Download video window: yt-dlp couldn't read the cookies of the browser chosen in Options; {browser} = its name, {error} = yt-dlp's words
                crate::i18n::t("yt-dlp couldn't read {browser}'s cookies: {error}"),
                &[("browser", browser), ("error", e)],
            )
        };
    }
    // A site's bot check turned yt-dlp away (#317).
    let plain = if lower.contains("anti-bot") {
        if store.is_some() {
            // i18n: the Download video window: the site's bot check (Cloudflare) turned yt-dlp away though it had the cookies of the browser chosen in Options; {browser} = its name
            return named(
                "The site's bot check (Cloudflare) refused yt-dlp, even with {browser}'s cookies. Use the Download video button over the video in {browser}, or try the page again later.",
            );
        }
        if sent {
            // i18n: the Download video window: the site's bot check (Cloudflare) turned yt-dlp away though the browser's extension passed the page's cookies
            "The site's bot check (Cloudflare) refused yt-dlp, even with your browser's cookies for this page. Try again later, or from another browser."
        } else {
            // i18n: the Download video window: the site's bot check (Cloudflare) turned yt-dlp away and no browser cookies were used
            "The site's bot check (Cloudflare) refused yt-dlp. If the page opens in your browser, choose that browser in Options → Sites Logins → Cookies for video sites and look the page up again, or use the Download video button over the video."
        }
    } else if mdm_tools::proc::untrusted_certificate(e) {
        // i18n: the Download video window: the site's certificate is trusted neither by yt-dlp nor by the computer (asked again with the computer's certificates, #955); usually an antivirus or a proxy that reads secure (HTTPS) connections signs sites with a certificate of its own
        "The site's certificate isn't trusted, by yt-dlp nor by this computer: an antivirus's web filter or a proxy that reads HTTPS may be in the way. Turn its HTTPS scanning off, or add its certificate to the computer's, then look the page up again."
    } else if lower.contains("drm") {
        // i18n: the Download video window: the page's video is copy-protected
        "This video is protected (DRM): no tool can save it."
    } else if lower.contains("unsupported url") {
        // i18n: the Download video window: yt-dlp found no video on the page
        "No video found on this page. Play the video, then use the Download video button over it."
    } else if lower.contains("sign in")
        || lower.contains("login")
        || lower.contains("private video")
    {
        if store.is_some() {
            // i18n: the Download video window: the site asked for a sign-in although yt-dlp had the cookies of the browser chosen in Options; {browser} = its name
            return named(
                "The site wants you signed in, even with {browser}'s cookies. Sign in to it in {browser}, then look the page up again.",
            );
        }
        if sent {
            // i18n: the Download video window: the site asked for a sign-in although the browser's extension passed the page's cookies
            "The site wants you signed in, even with your browser's cookies for this page. Sign in to it in your browser, then try again."
        } else {
            // i18n: the Download video window: the video needs the user to be signed in
            "This video needs you to be signed in. Use the Download video button over it in your browser, which passes your sign-in on, or choose your browser in Options → Sites Logins → Cookies for video sites."
        }
    } else {
        return e.to_owned();
    };
    crate::i18n::t(plain).to_owned()
}

/// A choice's size as its line shows it, and as a screen reader says it: the bytes of the
/// streams it downloads, or an estimate marked as one ("~532 MB", "about 532 MB"; #801).
pub(crate) fn choice_size(c: &Choice) -> (String, String) {
    let Some(bytes) = c.size else {
        // i18n: the video's file size is not known
        let unknown = crate::i18n::t("size unknown").to_owned();
        return (unknown.clone(), unknown);
    };
    let size = crate::dialogs::dialog_size(bytes);
    if !c.approx {
        return (size.clone(), size);
    }
    (
        crate::i18n::ltr(&format!("~{size}")),
        crate::i18n::fmt(
            // i18n: a file size that is only an estimate, as a screen reader says it in the Download video window's list of qualities; {size} is "532 MB"
            crate::i18n::t("about {size}"),
            &[("size", &size)],
        ),
    )
}

/// A choice's label in the user's language: the audio choice's words translated, the rest (a
/// resolution, a type) as they are, kept left to right.
pub(crate) fn choice_label(label: &str) -> String {
    if let Some(rest) = label.strip_prefix(mdm_tools::resolve::COMPATIBLE) {
        let height = rest.trim_start_matches([' ', '·']).trim();
        return crate::i18n::fmt(
            // i18n: a choice in the Download video window: the best H.264 video with AAC sound, an MP4 that every video editor, TV and phone plays; keep "(H.264 + AAC)" as it is; {height} is the resolution (1080p)
            crate::i18n::t("Most compatible (H.264 + AAC) · {height}"),
            &[("height", &crate::i18n::ltr(height))],
        );
    }
    if let Some(kind) = label.strip_prefix(mdm_tools::resolve::AUDIO_ORIGINAL) {
        return crate::i18n::fmt(
            // i18n: a choice in the Download video window: only the sound, the site's own file as it is (not converted); {type} is WEBM, M4A…
            crate::i18n::t("Audio only, original {type}"),
            &[("type", &crate::i18n::ltr(kind.trim()))],
        );
    }
    match label.strip_prefix(mdm_tools::resolve::AUDIO_ONLY) {
        Some(kind) => crate::i18n::fmt(
            // i18n: a choice in the Download video window: only the sound; {type} is MP3, M4A…
            crate::i18n::t("Audio only {type}"),
            &[("type", &crate::i18n::ltr(kind.trim()))],
        ),
        None => crate::i18n::ltr(label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_raw_formats_builds_the_exact_pair_in_either_click_order() {
        let video = mdm_tools::resolve::parse(br#"{"formats":[
            {"format_id":"137","ext":"mp4","height":1080,"vcodec":"avc1","acodec":"none","filesize":100},
            {"format_id":"251","ext":"webm","vcodec":"none","acodec":"opus","filesize":25},
            {"format_id":"18","ext":"mp4","height":360,"vcodec":"avc1","acodec":"mp4a"},
            {"format_id":"sb0","ext":"mhtml","vcodec":"none","acodec":"none"}
        ]}"#).unwrap();
        for order in [[0, 1], [1, 0]] {
            let mut code = String::new();
            for index in order {
                pick_format(&video, &mut code, index);
            }
            let choice = format_code_choice(&video, &code, "mkv").unwrap();
            assert_eq!(choice.selector, "137+251");
            assert_eq!(choice.ext, "mkv");
            assert_eq!(choice.size, Some(125));
            pick_format(&video, &mut code, 3);
            assert_eq!(code, "137+251", "storyboards cannot be selected");
        }
        assert!(format_code_choice(&video, "18+251", "mkv").is_none());
        assert!(format_code_choice(&video, "137/18", "mkv").is_none());
        assert_eq!(
            format_code_choice(&video, "251", "mkv").unwrap().ext,
            "webm"
        );
    }

    #[test]
    fn orbit_audio_choices_keep_their_container_and_explicit_config_overrides() {
        let original = mdm_tools::preferences::Preferences::default()
            .profile(false)
            .unwrap();
        let manual = audio_profile(original.clone());
        assert!(manual.value("--remux-video").is_none());
        assert!(!manual.base_args.iter().any(|a| a == "--remux-video"));
        let mut configured = original;
        configured
            .args
            .extend(["--remux-video".into(), "mka".into()]);
        let configured = audio_profile(configured);
        assert_eq!(configured.value("--remux-video"), Some("mka"));
        let saved: mdm_tools::config::Profile =
            serde_json::from_str(&serde_json::to_string(&manual).unwrap()).unwrap();
        assert!(
            audio_profile(saved)
                .resolved()
                .unwrap()
                .value("--remux-video")
                .is_none()
        );
    }

    #[test]
    fn orbit_video_presets_use_saved_quality_and_keep_advanced_formats_available() {
        let mut d = VideoDialog::blank(PathBuf::from("."));
        d.preferences = mdm_tools::preferences::Preferences::default();
        d.preferences.quality = 4;
        d.tools = Some((
            None,
            None,
            Some(mdm_tools::Installed {
                exe: PathBuf::from("ffmpeg.exe"),
                version: "8.0".into(),
            }),
        ));
        d.show_sample();
        assert!(!d.advanced);
        assert_eq!(d.chosen, 4);
        let choices = &d.video.as_ref().unwrap().1;
        assert_eq!(choices.len(), mdm_tools::preferences::QUALITIES.len());
        assert_eq!(choices[4].ext, "mkv");
        assert_eq!(
            choices[4].selector,
            "bestvideo[height<=720][vcodec^=avc1]+bestaudio / best[height<=720]"
        );
        d.pick("1080p");
        assert!(d.advanced);
        assert!(
            d.video
                .as_ref()
                .unwrap()
                .1
                .iter()
                .any(|c| c.selector.contains("299"))
        );
    }

    #[test]
    fn orbit_manual_video_snapshots_options_and_reuses_lookup_metadata() {
        let mut d = looked_up(false);
        d.preferences = mdm_tools::preferences::Preferences::default();
        d.advanced = false;
        d.page = "https://video.example/talk".into();
        d.looked_up = d.page.clone();
        let (video, _) = d.video.take().unwrap();
        d.set_video(video);
        d.kept = Some(mdm_tools::video::InfoSlot::new(std::time::Instant::now()));
        let Some(VideoOut::One(add)) = d.added() else {
            panic!("video")
        };
        assert!(add.preloaded.is_some());
        let profile = add.ytdlp.unwrap();
        assert!(profile.managed);
        assert!(profile.file.is_none());
        assert_eq!(profile.value("--merge-output-format"), Some("mkv"));
        assert!(profile.args.iter().any(|a| a == "--embed-info-json"));
        assert!(!profile.args.iter().any(|a| a == "--recode-video"));
        d.page = "https://video.example/other".into();
        let Some(VideoOut::One(add)) = d.added() else {
            panic!("video")
        };
        assert!(
            add.preloaded.is_none(),
            "an edited address invalidates cached metadata"
        );
    }

    /// The builder start_video uses must remember a kept row's origin after serde drops its
    /// request context; in-session private context and ordinary rows keep their own behavior.
    #[test]
    fn private_video_cookie_policy_survives_a_restart() {
        let page: url::Url = "https://videos.test/watch".parse().unwrap();
        for private in [false, true] {
            let mut item = crate::model::Item::new(
                1,
                page.clone(),
                PathBuf::from("Clip.mp4"),
                Category::Video,
            );
            item.video = Some(crate::model::VideoSpec {
                ytdlp: None,
                selector: "best".into(),
                subs: Default::default(),
                parts: vec![],
                named: false,
                duration: None,
            });
            item.private_window = private;
            item.context.private = private;
            item.context.cookies = Some("sid=private-secret".into());
            let saved = serde_json::to_string(&item).unwrap();
            assert!(!saved.contains("private-secret"));
            let reloaded: crate::model::Item = serde_json::from_str(&saved).unwrap();
            assert!(!reloaded.context.private);
            assert!(reloaded.context.cookies.is_none());
            let browser = from_item(&reloaded);
            assert_eq!(browser.private, private);
            let (args, file) = browser.args(page.as_str()).unwrap();
            assert!(file.is_none());
            assert_eq!(
                args.iter().any(|a| a == "--no-cookies-from-browser"),
                private
            );
            if private {
                let cmd = mdm_tools::proc::ytdlp(
                    std::path::Path::new("yt-dlp"),
                    &[],
                    &args,
                    page.as_str(),
                )
                .unwrap();
                assert!(!cmd.get_args().any(|a| a == "--cookies-from-browser"));
                assert!(!mdm_tools::proc::has_cookies(&args));
            } else {
                assert!(args.is_empty(), "normal rows still allow Options' fallback");
            }
        }
        let mut item =
            crate::model::Item::new(2, page.clone(), PathBuf::from("Clip.mp4"), Category::Video);
        item.context.private = true;
        assert!(
            !item.private_window,
            "also protect a memory-only private context"
        );
        assert!(
            from_item(&item)
                .args(page.as_str())
                .unwrap()
                .0
                .contains(&"--no-cookies-from-browser".into())
        );
        item.context.cookies = Some("sid=private".into());
        let browser = from_item(&item);
        let (args, file) = browser.args(page.as_str()).unwrap();
        assert!(file.is_some());
        assert!(args.contains(&"--cookies".into()));
        assert!(!args.contains(&"--no-cookies-from-browser".into()));
        assert!(
            from_browser(&item.context).private,
            "fresh private look-ups too"
        );
    }

    #[test]
    fn a_private_video_authentication_error_explains_its_cookie_policy() {
        let refusal = "ERROR: Private video. Sign in to watch.";
        let browser = mdm_tools::proc::FromBrowser {
            private: true,
            ..Default::default()
        };
        let message = request_error(refusal, &browser);
        assert!(message.starts_with(refusal), "{message}");
        assert!(message.contains("Private-window downloads do not use Cookies for video sites"));
        assert!(message.contains("private window"));
        assert!(!request_error(refusal, &Default::default()).contains("Private-window downloads"));
        assert_eq!(private_auth_error("HTTP Error 500"), None);
    }

    /// #359: a Dailymotion player embedded in another site is looked up at the video's own page,
    /// without that site as its Referer (Dailymotion answers 403); any other page keeps it.
    #[test]
    fn an_embedded_player_taken_to_its_page_drops_the_embedding_sites_referer() {
        let from = |page: &str| {
            let context = mdm_ipc::Command {
                referer: Some("https://news.test/article".into()),
                ..Default::default()
            };
            VideoDialog::for_page(PathBuf::from("v"), page, context)
        };
        let embed = from("https://geo.dailymotion.com/player/x1abc.html?video=x9xyz12");
        assert_eq!(embed.page, "https://www.dailymotion.com/video/x9xyz12");
        assert_eq!(embed.context().referer, None);
        let plain = from("https://player.vimeo.test/video/1");
        assert_eq!(
            plain.context().referer.as_deref(),
            Some("https://news.test/article")
        );
    }

    /// #317: Cloudflare's check in plain words, saying what to try for each kind of cookies had.
    #[test]
    fn a_bot_check_says_what_to_try() {
        let check = "yt-dlp failed: [generic] Got HTTP Error 403 caused by Cloudflare anti-bot challenge; try again with --extractor-args \"generic:impersonate\"";
        let none = lookup_error(check, false, None);
        assert!(none.contains("bot check"), "{none}");
        assert!(none.contains("Cookies for video sites"), "{none}");
        let sent = lookup_error(check, true, None);
        assert!(sent.contains("even with your browser's cookies"), "{sent}");
        let firefox = mdm_tools::proc::BrowserCookies::new("firefox", "").unwrap();
        let chrome_had = lookup_error(check, false, Some(&firefox));
        assert!(
            chrome_had.contains("even with Firefox's cookies"),
            "{chrome_had}"
        );
        assert_eq!(
            lookup_error("yt-dlp failed: Video unavailable", false, None),
            "yt-dlp failed: Video unavailable"
        );
    }

    /// #955: a certificate trusted neither by yt-dlp nor by the computer (asked again with its
    /// certificates) says what reads HTTPS on the way; an expired one stays yt-dlp's own words.
    #[test]
    fn an_untrusted_certificate_says_what_may_be_in_the_way() {
        let untrusted = "yt-dlp failed: [youtube] X6ek6j1gTQA: Unable to download API page: [SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get local issuer certificate (_ssl.c:1085) (caused by CertificateVerifyError('[SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get local issuer certificate (_ssl.c:1085)'))";
        let said = lookup_error(untrusted, false, None);
        assert!(said.contains("antivirus's web filter"), "{said}");
        let expired = "yt-dlp failed: [generic] Unable to download webpage: [SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: certificate has expired (_ssl.c:1006)";
        assert_eq!(lookup_error(expired, false, None), expired);
    }

    /// #229: yt-dlp's refusals and its trouble with a browser's cookies, in MDM's words, saying
    /// which cookies it had.
    #[test]
    fn cookie_and_sign_in_errors_in_plain_words() {
        let bot = "yt-dlp failed: [youtube] A1WG3x-Ebpk: Sign in to confirm you're not a bot. Use --cookies-from-browser or --cookies for the authentication.";
        assert!(lookup_error(bot, false, None).contains("Cookies for video sites"));
        assert!(lookup_error(bot, true, None).contains("even with your browser's cookies"));
        let firefox = mdm_tools::proc::BrowserCookies::new("firefox", "").unwrap();
        let chrome = mdm_tools::proc::BrowserCookies::new("chrome", "").unwrap();
        assert_eq!(
            lookup_error(bot, false, Some(&firefox)),
            "The site wants you signed in, even with Firefox's cookies. Sign in to it in Firefox, then look the page up again."
        );
        // Chrome's app-bound cookies: only a warning, kept on the error (proc::with_cookie_trouble).
        let v20 = format!("{bot} (unknown cookie version: \"b'v20'\")");
        let said = lookup_error(&v20, false, Some(&chrome));
        if cfg!(windows) {
            assert!(
                said.starts_with("Chrome keeps its cookies locked"),
                "{said}"
            );
        } else {
            // No app-bound encryption elsewhere: yt-dlp's own words.
            assert!(
                said.starts_with("yt-dlp couldn't read Chrome's cookies: "),
                "{said}"
            );
        }
        // Linux: the key wasn't in the keyring.
        let v11 = format!("{bot} (cannot decrypt v11 cookies: no key found)");
        let said = lookup_error(&v11, false, Some(&chrome));
        if cfg!(windows) {
            assert!(
                said.starts_with("Chrome keeps its cookies locked"),
                "{said}"
            );
        } else {
            assert!(said.contains("system's keyring"), "{said}");
        }
        assert!(
            lookup_error(
                "yt-dlp failed: failed to load cookies (Could not copy Chrome cookie database. See  https://github.com/yt-dlp/yt-dlp/issues/7271  for more info)",
                false,
                Some(&chrome)
            )
            .starts_with("yt-dlp can't read Chrome's cookies while Chrome is open")
        );
        assert!(
            lookup_error(
                "yt-dlp failed: failed to load cookies (could not find firefox cookies database in 'C:\\p')",
                false,
                Some(&firefox)
            )
            .starts_with("yt-dlp found no cookies of Firefox")
        );
        let keyring = lookup_error(
            "yt-dlp failed: Sign in (failed to read from keyring)",
            false,
            Some(&chrome),
        );
        assert!(keyring.contains("system's keyring"), "{keyring}");
        // Anything else stays yt-dlp's own words.
        assert_eq!(
            lookup_error("yt-dlp failed: Video unavailable", false, Some(&firefox)),
            "yt-dlp failed: Video unavailable"
        );
    }

    fn sub(code: &str, automatic: bool) -> SubChoice {
        SubChoice {
            code: code.into(),
            name: code.into(),
            automatic,
        }
    }

    fn want(langs: &[&str], automatic: bool) -> Subs {
        Subs {
            langs: langs.iter().map(|l| l.to_string()).collect(),
            automatic,
            embed: false,
        }
    }

    /// The Subtitles list (#68): None, All the uploader's when they gave two or more, then each
    /// language, the automatic ones marked.
    #[test]
    fn subtitle_list_entries() {
        let subs = [sub("de", false), sub("fr", false), sub("en", true)];
        assert_eq!(
            sub_items(&subs, true),
            ["None", "All the uploader's", "de", "fr", "en, automatic"]
        );
        assert_eq!(sub_items(&subs[2..], true), ["None", "en, automatic"]);
        assert_eq!(sub_items(&[], true), ["None on this page"]);
        assert_eq!(sub_items(&[], false), ["None"]);
    }

    /// The last video's choice finds its place in the next video's list: the same language
    /// (the uploader's first), a variant of it, every uploader's again, or none.
    #[test]
    fn the_last_choice_is_offered_again() {
        let subs = [sub("de", false), sub("pt-BR", false), sub("en", true)];
        assert_eq!(pick_for(&subs, &Subs::default()), 0);
        assert_eq!(pick_for(&subs, &want(&[ALL_UPLOADERS], false)), 1);
        assert_eq!(pick_for(&subs, &want(&["de"], false)), 2);
        assert_eq!(pick_for(&subs, &want(&["pt"], false)), 3, "pt for pt-BR");
        assert_eq!(
            pick_for(&subs, &want(&["en"], false)),
            4,
            "the automatic one"
        );
        assert_eq!(pick_for(&subs, &want(&["ja"], false)), 0);
        // One uploader's language only: "all" stands for it.
        let one = [sub("fr", false), sub("en", true)];
        assert_eq!(pick_for(&one, &want(&[ALL_UPLOADERS], false)), 1);
    }

    /// Each entry stands for what yt-dlp is asked.
    #[test]
    fn entries_ask_for_their_subtitles() {
        let mut d = VideoDialog::blank(PathBuf::from("."));
        d.preferred_subtitles = false;
        d.subs = vec![sub("de", false), sub("fr", false), sub("en-orig", true)];
        d.embed = true;
        let mut at = |i: usize| {
            d.sub_pick = i;
            d.chosen_subs()
        };
        assert!(at(0).langs.is_empty());
        assert_eq!(at(1).langs, [ALL_UPLOADERS]);
        assert_eq!(at(3).langs, ["fr"]);
        let auto = at(4);
        assert_eq!(auto.langs, ["en-orig"]);
        assert!(auto.automatic && auto.embed);
    }

    /// #345: a stream the browser's button sent (its manifest, which yt-dlp names after its
    /// address) is saved under the title of the page that played it; a video yt-dlp titled
    /// itself keeps yt-dlp's title.
    #[test]
    fn a_streams_file_is_named_after_its_page() {
        const STREAM: &str = r#"{"title": "master", "formats": [
            {"format_id": "hls-720", "ext": "mp4", "height": 720, "vcodec": "avc1", "acodec": "mp4a"}]}"#;
        let video = || mdm_tools::resolve::parse(STREAM.as_bytes()).unwrap();
        let src = "https://cdn.test/v/hls/master.m3u8";
        let context = mdm_ipc::Command {
            video_page: Some(src.into()),
            video_title: Some("A talk: about download managers? ".into()),
            ..Default::default()
        };
        let mut d = VideoDialog::for_page(PathBuf::from("."), src, context.clone());
        d.set_video(video());
        assert_eq!(d.name, "A talk_ about download managers_.mkv");
        // No page title came (an older extension, or a page yt-dlp read itself): yt-dlp's.
        let mut d = VideoDialog::for_page(PathBuf::from("."), src, Default::default());
        d.set_video(video());
        assert_eq!(d.name, "master.mkv");
        // The address edited: the browser's title is not for it.
        let mut d = VideoDialog::for_page(PathBuf::from("."), src, context.clone());
        d.page = "https://cdn.test/other/index.m3u8".into();
        d.set_video(video());
        assert_eq!(d.name, "master.mkv");
        // A page yt-dlp read itself: its title, not the browser's.
        const TITLED: &str = r#"{"title": "Its own title", "formats": [
            {"format_id": "18", "ext": "mp4", "height": 360, "vcodec": "avc1", "acodec": "mp4a"}]}"#;
        let page = "https://videos.test/watch?v=1";
        let mut d = VideoDialog::for_page(PathBuf::from("."), page, context);
        d.set_video(mdm_tools::resolve::parse(TITLED.as_bytes()).unwrap());
        assert_eq!(d.name, "Its own title.mkv");
    }

    /// The browser's quality menu adds its choice as this window would, untouched: the same
    /// file name, and the subtitles it preselects from the last video's choice.
    #[test]
    fn the_quality_menu_adds_what_the_window_would() {
        const TALK: &str = r#"{"title": "A talk: in Lisbon", "language": "en", "duration": 60, "formats": [
            {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 29000000},
            {"format_id": "43", "ext": "webm", "height": 480, "vcodec": "vp8", "acodec": "vorbis"},
            {"format_id": "18", "ext": "mp4", "height": 360, "vcodec": "avc1", "acodec": "mp4a", "filesize": 61000000}],
            "subtitles": {"de": [{"name": "German"}], "fr": [{"name": "French"}]}}"#;
        let v = mdm_tools::resolve::parse(TALK.as_bytes()).unwrap();
        let page = "https://videos.test/talk";
        let with_title = mdm_ipc::Command {
            video_page: Some(page.into()),
            video_title: Some(" The page's title ".into()),
            ..Default::default()
        };
        let embed = |mut s: Subs| {
            s.embed = true;
            s
        };
        for merge in [false, true] {
            let choices = mdm_tools::resolve::choices(&v, merge);
            assert!(choices.len() >= 2);
            for context in [mdm_ipc::Command::default(), with_title.clone()] {
                for pref in [
                    Subs::default(),
                    want(&["de"], false),
                    embed(want(&["fr"], false)),
                    want(&["ja"], false),
                    embed(want(&[ALL_UPLOADERS], false)),
                ] {
                    let mut d = VideoDialog::for_page(PathBuf::from("."), page, context.clone())
                        .with_subs(pref.clone());
                    // As set_video leaves it, with or without ffmpeg (not this computer's):
                    // yt-dlp's title is the page's own, which no browser title replaces.
                    assert_eq!(page_title(&context, &v, page), None);
                    d.video = Some((v.clone(), choices.clone()));
                    d.subs = mdm_tools::resolve::sub_choices(&v, crate::i18n::code());
                    d.sub_pick = pick_for(&d.subs, &d.pref);
                    for (i, c) in choices.iter().enumerate() {
                        d.chosen = i;
                        d.name_from_choice();
                        assert_eq!(
                            subs_for_choice(c, &d.chosen_subs()),
                            default_subs(&v, c, &pref),
                            "{pref:?} {c:?}"
                        );
                        assert_eq!(
                            d.name,
                            mdm_tools::preferences::filename(
                                &d.preferences.filename_template,
                                &v.title,
                                &v.naming,
                                if mdm_tools::resolve::is_audio(c) {
                                    &c.ext
                                } else {
                                    &d.preferences.container
                                }
                            )
                        );
                    }
                }
            }
        }
        let choices = mdm_tools::resolve::choices(&v, false);
        let mp4 = choices.iter().find(|c| c.ext == "mp4").unwrap();
        let webm = choices.iter().find(|c| c.ext == "webm").unwrap();
        let audio = choices
            .iter()
            .find(|c| mdm_tools::resolve::is_audio(c))
            .unwrap();
        assert_eq!(default_subs(&v, mp4, &want(&["de"], false)).langs, ["de"]);
        let fr = default_subs(&v, webm, &embed(want(&["fr"], false)));
        assert_eq!(fr.langs, ["fr"]);
        assert!(!fr.embed, "a WebM can't hold them: beside it");
        assert!(default_subs(&v, mp4, &embed(want(&["fr"], false))).embed);
        assert!(
            default_subs(&v, audio, &want(&["de"], false))
                .langs
                .is_empty(),
            "none with the sound alone"
        );
        assert!(
            default_subs(&v, mp4, &want(&["ja"], false))
                .langs
                .is_empty(),
            "not on this page: none"
        );
        assert_eq!(file_name(&v.title, mp4), "A talk_ in Lisbon.mp4");
        // Named after its address (`talk`): the page's title, trimmed (#345).
        let hashed = Video {
            naming: Default::default(),
            title: "talk".into(),
            ..v.clone()
        };
        assert_eq!(
            page_title(&with_title, &hashed, page),
            Some("The page's title")
        );
        assert_eq!(
            page_title(&mdm_ipc::Command::default(), &hashed, page),
            None
        );
    }

    /// #1351: a player embedded in a frame of another site's page (VOE's on an episode's page),
    /// which yt-dlp has no reader for: its generic reader names the video by the frame's own
    /// title, and the tab's names it instead. A site's reader's title (YouTube's, in an
    /// article's frame) stays.
    #[test]
    fn an_embedded_player_yt_dlp_reads_generically_is_named_by_the_tabs_title() {
        let frame = "https://player.test/e/dzA1YmtiODgyWDhCQi9NMW85K1Nq";
        let answer = |key: &str| {
            format!(
                r#"{{"title": "Watch file.mp4 - VOE | Content Delivery Network (CDN) & Video Cloud",
                    "extractor_key": "{key}",
                    "formats": [{{"format_id": "0", "ext": "mp4", "url": "https://cdn.test/v.mp4"}}]}}"#
            )
        };
        let context = mdm_ipc::Command {
            video_title: Some("Renegade Immortal Episodio 167 Sub Español".into()),
            ..Default::default()
        };
        let generic = mdm_tools::resolve::parse(answer("Generic").as_bytes()).unwrap();
        assert!(generic.page_titled);
        let title = page_title(&context, &generic, frame).unwrap();
        assert_eq!(title, "Renegade Immortal Episodio 167 Sub Español");
        let mp4 = Choice {
            ext: "mp4".into(),
            ..Default::default()
        };
        assert_eq!(
            file_name(title, &mp4),
            "Renegade Immortal Episodio 167 Sub Español.mp4"
        );
        // A page's plain `<video>` in the frame: yt-dlp's HTML5 reader, the page's title too.
        let html5 = mdm_tools::resolve::parse(answer("HTML5MediaEmbed").as_bytes()).unwrap();
        assert_eq!(
            page_title(&context, &html5, frame),
            Some("Renegade Immortal Episodio 167 Sub Español")
        );
        // Nothing from the browser: yt-dlp's title stays.
        assert_eq!(
            page_title(&mdm_ipc::Command::default(), &generic, frame),
            None
        );
        // A site yt-dlp has a reader for: its title stays.
        let known = mdm_tools::resolve::parse(answer("Youtube").as_bytes()).unwrap();
        assert!(!known.page_titled);
        assert_eq!(page_title(&context, &known, frame), None);
    }

    /// #622: a Dropbox page is titled with the file's name, extension and all; the video named
    /// by it got the choice's extension once more (`Episode 31.mp4.mp4`).
    #[test]
    fn a_page_titled_with_the_file_name_names_it_once() {
        let page = "https://www.dropbox.com/s/abc/Episode%2031.mp4?dl=0";
        let v = Video {
            naming: Default::default(),
            title: "Episode 31".into(),
            duration: None,
            formats: Vec::new(),
            subtitles: Vec::new(),
            automatic: Vec::new(),
            language: None,
            direct: false,
            needs_js_runtime: false,
            page_titled: false,
        };
        let mp4 = Choice {
            ext: "mp4".into(),
            ..Default::default()
        };
        for (title, want) in [
            ("Episode 31.mp4", "Episode 31"),
            (" Episode 31.MKV ", "Episode 31"),
            ("Episode 31.m4a", "Episode 31"),
            // Not a media file's extension: a title as it is.
            ("Episode 31. The end", "Episode 31. The end"),
            ("Report v2.0", "Report v2.0"),
            (".mp4", ".mp4"),
        ] {
            let context = mdm_ipc::Command {
                video_title: Some(title.into()),
                ..Default::default()
            };
            let got = page_title(&context, &v, page).unwrap();
            assert_eq!(got, want, "{title}");
            if want == "Episode 31" {
                assert_eq!(file_name(got, &mp4), "Episode 31.mp4");
            }
        }
    }

    /// A film whose video plays in another site's player, in a frame (Vimeo's on a film site),
    /// as a browser saw it (`tests/data/embedded-player-title.json`): yt-dlp knows the stream
    /// the button sends only by its address (`playlist`), and the title the extension sends
    /// with it (the tab's, without the site's name: tools/extension/test.mjs) names the video
    /// in the window and its file. It was the player frame's own, "Before The - Short Film from
    /// Justin Barry on Vimeo".
    #[test]
    fn a_stream_from_an_embedded_player_is_named_by_the_pages_title() {
        let film: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/embedded-player-title.json")).unwrap();
        let text = |k: &str| film[k].as_str().unwrap().to_owned();
        let v = mdm_tools::resolve::parse(film["ytdlp"].to_string().as_bytes()).unwrap();
        assert_eq!(v.title, "playlist");
        let context = mdm_ipc::Command {
            video_title: Some(text("sent")),
            ..Default::default()
        };
        let title = page_title(&context, &v, &text("stream")).unwrap();
        assert_eq!(title, text("sent"));
        let best = &mdm_tools::resolve::choices(&v, true)[0];
        assert_eq!(best.height, Some(2160));
        assert_eq!(file_name(title, best), text("file"));
    }

    /// #661: a page's address from the clipboard comes a frame or more after the window opens:
    /// it fills an empty box only, and only with a web address.
    #[test]
    fn the_clipboard_fills_only_an_empty_page_box() {
        let (video, _) = dirs();
        let mut d = VideoDialog::blank(video.clone());
        d.fill_from_clipboard(
            " https://video.example/talk 
",
        );
        assert_eq!(d.page, "https://video.example/talk");
        d.fill_from_clipboard("https://video.example/other");
        assert_eq!(
            d.page, "https://video.example/talk",
            "not over what is there"
        );
        let mut d = VideoDialog::blank(video);
        d.fill_from_clipboard("ftp://files.example/a.mp4");
        assert_eq!(d.page, "");
    }

    /// The Video and Music categories' folders (not made: only their names are compared).
    fn dirs() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join("mdm-611");
        (base.join("Video"), base.join("Music"))
    }

    /// Download video as the app opens it (Save to: Video's folder, Music's offered), a video
    /// with a sound-only choice and video ones looked up, without yt-dlp or ffmpeg.
    fn looked_up(music: bool) -> VideoDialog {
        const SAMPLE: &str = r#"{"title": "A talk", "duration": 60, "formats": [
            {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 2900},
            {"format_id": "18", "ext": "mp4", "height": 360, "vcodec": "avc1", "acodec": "mp4a", "filesize": 6100}]}"#;
        let (video, music_dir) = dirs();
        let mut d = VideoDialog::for_page(video, "https://video.example/talk", Default::default())
            .with_music_folder(music.then_some(music_dir));
        // No helpers looked for on this computer: no merged qualities.
        d.tools = Some((None, None, None));
        d.advanced = true;
        d.preferences.container = "mp4".into();
        d.preferred_subtitles = false;
        d.looked_up = d.page.clone();
        d.set_video(mdm_tools::resolve::parse(SAMPLE.as_bytes()).unwrap());
        d
    }

    /// The choices' rows: (a sound-only one, a video one).
    fn rows(d: &VideoDialog) -> (usize, usize) {
        let choices = &d.video.as_ref().unwrap().1;
        let sound = choices.iter().position(mdm_tools::resolve::is_audio);
        let video = choices
            .iter()
            .position(|c| !mdm_tools::resolve::is_audio(c));
        (sound.unwrap(), video.unwrap())
    }

    /// A row clicked in the choices list, as the list does it.
    fn choose(d: &mut VideoDialog, row: usize) {
        d.chosen = row;
        d.name_from_choice();
    }

    /// What Download Now adds for a single video: (its folder, its category).
    fn one(d: &VideoDialog) -> (PathBuf, Category) {
        match d.added() {
            Some(VideoOut::One(add)) => (add.dest.parent().unwrap().to_owned(), add.category),
            _ => panic!("a video is added"),
        }
    }

    /// #1076: the video a page declares is what the download keeps (its address, the page as its
    /// Referer, the browser's User-Agent, not the page's Cookie header), so the download's own
    /// look-up, when it starts, finds that one again; until the address is edited.
    #[test]
    fn a_declared_video_is_added_at_its_own_address() {
        let page = "https://films.example/films/before";
        let mut d = looked_up(false);
        d.page = page.into();
        d.from_browser = page.into();
        d.looked_up = page.into();
        d.context = mdm_ipc::Command {
            user_agent: Some("UA".into()),
            cookies: Some("session=1".into()),
            referer: Some("https://search.example/".into()),
            ..Default::default()
        };
        d.declared = Some(Declared {
            address: "https://player.vimeo.com/video/1244858801".into(),
            page: page.into(),
            name: Some("Before".into()),
        });
        let added = |d: &VideoDialog| match d.added() {
            Some(VideoOut::One(add)) => add,
            _ => panic!("a video is added"),
        };
        let add = added(&d);
        assert_eq!(
            add.page.as_str(),
            "https://player.vimeo.com/video/1244858801"
        );
        assert_eq!(add.context.referer.as_deref(), Some(page));
        assert_eq!(add.context.user_agent.as_deref(), Some("UA"));
        assert_eq!(add.context.cookies, None);
        // An address edited since: that address, as typed.
        d.page = "https://films.example/films/other".into();
        let add = added(&d);
        assert_eq!(add.page.as_str(), "https://films.example/films/other");
        assert_eq!(add.context.referer, None);
    }

    /// #611: a sound-only choice saves in the Music category's folder and is listed under Music;
    /// a video choice again goes back to Video's.
    #[test]
    fn a_sound_only_choice_saves_in_music_and_back() {
        let (video, music) = dirs();
        let mut d = looked_up(true);
        let (sound, film) = rows(&d);
        choose(&mut d, film);
        assert_eq!(d.save_folder(), video);
        assert_eq!(one(&d), (video.clone(), Category::Video));
        choose(&mut d, sound);
        assert_eq!(
            d.save_folder(),
            music,
            "Save to follows a sound-only choice"
        );
        assert_eq!(one(&d), (music.clone(), Category::Music));
        assert_eq!(d.category(), Category::Music, "the folder kept is Music's");
        choose(&mut d, film);
        assert_eq!(d.save_folder(), video, "and goes back for a video");
        assert_eq!(one(&d), (video, Category::Video));
    }

    /// #611: a folder the user typed or picked stays whatever is chosen; a sound-only download
    /// into it is still listed under Music.
    #[test]
    fn a_folder_the_user_chose_stays() {
        let mut d = looked_up(true);
        let (sound, film) = rows(&d);
        let own = std::env::temp_dir().join("mdm-611").join("Talks");
        d.folder = own.display().to_string();
        choose(&mut d, sound);
        assert_eq!(d.save_folder(), own);
        assert_eq!(one(&d), (own.clone(), Category::Music));
        choose(&mut d, film);
        assert_eq!(d.save_folder(), own);
        assert_eq!(one(&d), (own, Category::Video));
    }

    /// The Music category deleted (#93): sound-only goes where video does, as before.
    #[test]
    fn without_a_music_category_sound_goes_as_video() {
        let (video, _) = dirs();
        let mut d = looked_up(false);
        let (sound, _) = rows(&d);
        choose(&mut d, sound);
        assert_eq!(d.save_folder(), video);
        assert_eq!(one(&d), (video, Category::Video));
    }

    /// #611: a playlist downloaded as sound only goes into the Music category's folder (then the
    /// list's own folder) and each of its videos is listed under Music; a video quality, Video's.
    #[test]
    fn a_playlist_as_sound_saves_in_music() {
        let (video, music) = dirs();
        let many = |d: &VideoDialog| match d.added() {
            Some(VideoOut::Many(add)) => (add.folder.parent().unwrap().to_owned(), add.category),
            _ => panic!("a playlist is added"),
        };
        // "Audio only, original" (8), then "Up to 1080p" (4).
        for (pick, folder, category) in
            [("8", music, Category::Music), ("4", video, Category::Video)]
        {
            let mut d = looked_up(true);
            d.show_sample_list(pick);
            // As each frame of the window does.
            d.follow_sound();
            assert_eq!(d.save_folder(), folder, "quality {pick}");
            assert_eq!(many(&d), (folder, category), "quality {pick}");
        }
    }

    /// #1439: the most compatible choice is said in the user's language, its height as data.
    #[test]
    fn the_most_compatible_choice_is_labelled() {
        let l = choice_label("Most compatible (H.264 + AAC) · 1080p");
        assert!(l.starts_with("Most compatible (H.264 + AAC) · "), "{l}");
        assert!(l.contains("1080p") && !l.contains("·  "), "{l}");
        // The sample's list has it second, picked by its name for the screenshots.
        let mut d = VideoDialog::blank(PathBuf::from("."));
        // ffmpeg there (never looked for on this computer): the merged qualities.
        let ffmpeg = mdm_tools::Installed {
            version: "test".into(),
            exe: PathBuf::from("ffmpeg"),
        };
        d.tools = Some((None, None, Some(ffmpeg)));
        d.show_sample();
        d.pick("most");
        let (_, choices) = d.video.as_ref().unwrap();
        assert_eq!(d.chosen, 1);
        assert_eq!(choices[1].label, "Most compatible (H.264 + AAC) · 1080p");
        assert!(choices[1].selector.starts_with("299+140/"));
    }
}
