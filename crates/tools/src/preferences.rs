//! Persisted yt-dlp preferences. Codecs select source streams; containers never request encoding.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, path::Path};

pub const HEIGHTS: [Option<u32>; 9] = [
    None,
    Some(2160),
    Some(1440),
    Some(1080),
    Some(720),
    Some(480),
    Some(360),
    Some(240),
    Some(144),
];
pub const QUALITIES: [&str; 9] = [
    "Best available",
    "Up to 2160p (4K)",
    "Up to 1440p",
    "Up to 1080p",
    "Up to 720p",
    "Up to 480p",
    "Up to 360p",
    "Up to 240p",
    "Up to 144p",
];
pub const CONTAINERS: [&str; 3] = ["mkv", "mp4", "webm"];
pub const CODECS: [&str; 4] = ["Any available codec", "H.264 / AVC", "VP9", "AV1"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub use_config: bool,
    pub quality: usize,
    pub container: String,
    pub codec: usize,
    pub subtitle_languages: String,
    pub automatic_subtitles: bool,
    pub embed_subtitles: bool,
    pub embed_thumbnail: bool,
    pub embed_chapters: bool,
    pub embed_metadata: bool,
    pub embed_info_json: bool,
    pub filename_template: String,
    // Kept for installations configured before Deno had its own program entry.
    pub(crate) deno_path: String,
    pub remote_ejs: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            use_config: false,
            quality: 0,
            container: "mkv".into(),
            codec: 1,
            subtitle_languages: "en.*".into(),
            automatic_subtitles: true,
            embed_subtitles: true,
            embed_thumbnail: true,
            embed_chapters: true,
            embed_metadata: true,
            embed_info_json: true,
            filename_template: "%(title)s.%(ext)s".into(),
            deno_path: String::new(),
            remote_ejs: true,
        }
    }
}

impl Preferences {
    pub fn read() -> Self {
        crate::tools_dir().map_or_else(Self::default, |dir| Self::read_from(&dir))
    }
    pub(crate) fn read_from(dir: &Path) -> Self {
        std::fs::read(dir.join("video-preferences.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> io::Result<()> {
        self.validate()?;
        let dir = crate::tools_dir()
            .ok_or_else(|| io::Error::other("Orbit's tools folder is unavailable"))?;
        let mut preferences = self.clone();
        preferences.deno_path = Self::read_from(&dir).deno_path;
        preferences.save_to(&dir)
    }
    pub(crate) fn save_to(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        craft_atomic::atomic_write(
            &dir.join("video-preferences.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.quality >= HEIGHTS.len()
            || self.codec >= CODECS.len()
            || !CONTAINERS.contains(&self.container.as_str())
            || (self.container == "webm" && self.codec == 1)
        {
            return Err(io::Error::other(
                "Choose a supported quality, container and source codec combination",
            ));
        }
        validate_template(&self.filename_template)?;
        if self.subtitle_languages.len() > 256
            || self.subtitle_languages.contains(['\r', '\n', '\0'])
        {
            return Err(io::Error::other(
                "Enter subtitle language codes separated by commas",
            ));
        }
        Ok(())
    }
    pub fn selector(&self, quality: usize) -> String {
        let height = HEIGHTS
            .get(quality)
            .copied()
            .flatten()
            .map(|h| format!("[height<={h}]"))
            .unwrap_or_default();
        let codec = match self.codec {
            1 => "[vcodec^=avc1]",
            2 => "[vcodec^=vp9]",
            3 => "[vcodec^=av01]",
            _ => "",
        };
        let container = if self.container == "webm" {
            "[ext=webm]"
        } else {
            ""
        };
        // Prefer separate streams with the requested codec, then a single-file fallback.
        format!(
            "bestvideo{height}{codec}{container}+bestaudio{} / best{height}{container}",
            if self.container == "webm" {
                "[ext=webm]"
            } else {
                ""
            }
        )
    }
    pub(crate) fn runtime_args(&self) -> Vec<String> {
        if self.remote_ejs {
            vec!["--remote-components".into(), "ejs:github".into()]
        } else {
            Vec::new()
        }
    }

    pub fn profile(&self, use_file: bool) -> io::Result<crate::config::Profile> {
        let mut args = self.runtime_args();
        args.extend([
            "--merge-output-format".into(),
            self.container.clone(),
            "--remux-video".into(),
            self.container.clone(),
        ]);
        for (on, yes, no) in [
            (
                self.embed_thumbnail && self.container != "webm",
                "--embed-thumbnail",
                "--no-embed-thumbnail",
            ),
            (
                self.embed_metadata,
                "--embed-metadata",
                "--no-embed-metadata",
            ),
            (
                self.embed_info_json,
                "--embed-info-json",
                "--no-embed-info-json",
            ),
            (
                self.embed_chapters,
                "--embed-chapters",
                "--no-embed-chapters",
            ),
        ] {
            args.push(if on { yes } else { no }.into());
        }
        let mut profile = if use_file {
            let path = crate::config::path().ok_or_else(|| {
                io::Error::other("Select a yt-dlp configuration file in Preferences > yt-dlp")
            })?;
            crate::config::load(&path)?
        } else {
            if !self.subtitle_languages.trim().is_empty() {
                args.extend([
                    "--write-subs".into(),
                    "--sub-langs".into(),
                    self.subtitle_languages.trim().into(),
                    "--convert-subs".into(),
                    "srt".into(),
                ]);
                if self.automatic_subtitles {
                    args.push("--write-auto-subs".into());
                }
            }
            args.push(
                if self.embed_subtitles {
                    "--embed-subs"
                } else {
                    "--no-embed-subs"
                }
                .into(),
            );
            crate::config::Profile {
                name_template: self.filename_template.clone(),
                ..Default::default()
            }
        };
        profile.base_args = args.clone();
        args.extend(profile.args);
        profile.args = args;
        profile.managed = true;
        Ok(profile)
    }
}

pub fn validate_template(template: &str) -> io::Result<()> {
    if template.is_empty()
        || template.len() > 180
        || template.contains(['\\', '/', ':', '\r', '\n', '\0'])
        || template.contains("..")
    {
        return Err(io::Error::other(
            "Use a filename template without folders or path separators",
        ));
    }
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        rest = &rest[at..];
        let Some(field) = [
            "%(title)s",
            "%(ext)s",
            "%(id)s",
            "%(uploader)s",
            "%(upload_date)s",
        ]
        .into_iter()
        .find(|f| rest.starts_with(f)) else {
            return Err(io::Error::other(
                "Supported filename fields: title, ext, id, uploader and upload_date",
            ));
        };
        rest = &rest[field.len()..];
    }
    Ok(())
}

pub fn filename(
    template: &str,
    title: &str,
    fields: &BTreeMap<String, String>,
    ext: &str,
) -> String {
    let mut result = template.to_owned();
    for field in ["title", "id", "uploader", "upload_date", "ext"] {
        let value = match field {
            "title" => title,
            "ext" => ext,
            _ => fields.get(field).map_or("NA", String::as_str),
        };
        result = result.replace(
            &format!("%({field})s"),
            &odm_engine::naming::sanitize(value),
        );
    }
    if !result.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
        result.push_str(&format!(".{ext}"));
    }
    odm_engine::naming::sanitize(&result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orbit_ytdlp_presets_select_source_codecs_without_encoding() {
        let p = Preferences::default();
        assert_eq!(p.container, "mkv");
        assert_eq!(
            p.selector(4),
            "bestvideo[height<=720][vcodec^=avc1]+bestaudio / best[height<=720]"
        );
        let profile = p.profile(false).unwrap();
        assert!(profile.args.contains(&"--embed-thumbnail".into()));
        assert!(profile.args.contains(&"--embed-info-json".into()));
        assert!(!profile.args.contains(&"--recode-video".into()));
        assert_eq!(profile.value("--merge-output-format"), Some("mkv"));
        assert_eq!(profile.value("--convert-subs"), Some("srt"));
    }
    #[test]
    fn orbit_ytdlp_filename_templates_cannot_escape_the_destination() {
        assert!(validate_template("../%(title)s.%(ext)s").is_err());
        assert!(validate_template("C:\\elsewhere\\x").is_err());
        assert!(validate_template("%(unknown)s").is_err());
        assert_eq!(
            filename(
                "%(title)s [%(id)s].%(ext)s",
                "A/B",
                &BTreeMap::from([("id".into(), "xyz".into())]),
                "mkv"
            ),
            "A_B [xyz].mkv"
        );
    }
}
