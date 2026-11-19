//! User yt-dlp preferences, with Orbit retaining its download, process and output protocols.
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Profile {
    pub file: Option<PathBuf>,
    #[serde(skip)]
    pub args: Vec<String>,
    pub base_args: Vec<String>,
    pub managed: bool,
    pub name_template: String,
}

thread_local! {
    static CURRENT: std::cell::RefCell<Option<Profile>> = const { std::cell::RefCell::new(None) };
}

/// A profile is local to one lookup or job, never shared with concurrent downloads.
pub fn with_profile<T>(profile: Profile, work: impl FnOnce() -> T) -> T {
    struct Reset(Option<Profile>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let previous = CURRENT.with(|c| c.replace(Some(profile)));
    let _reset = Reset(previous);
    work()
}

pub fn path() -> Option<PathBuf> {
    let custom = crate::tools_dir().and_then(|d| crate::chosen::config_file(&d));
    custom.or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .map(|h| h.join("yt-dlp.conf"))
            .filter(|p| p.is_file())
    })
}

pub fn active() -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_some_and(|p| p.file.is_some()))
}
pub fn managed() -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_some_and(|p| p.managed))
}
pub fn profile() -> io::Result<Profile> {
    CURRENT.with(|c| c.borrow().clone()).map_or_else(
        || crate::preferences::Preferences::read().profile(false),
        Profile::resolved,
    )
}
pub fn load(path: &Path) -> io::Result<Profile> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 1 << 20 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "yt-dlp config is too large",
        ));
    }
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tokens = tokens(text)?;
    let name_template = tokens
        .windows(2)
        .rev()
        .find(|p| {
            matches!(p[0].as_str(), "-o" | "--output")
                && crate::preferences::validate_template(&p[1]).is_ok()
        })
        .map_or_else(|| "%(title)s.%(ext)s".into(), |p| p[1].clone());
    Ok(Profile {
        file: Some(path.to_owned()),
        args: preferences(&tokens),
        managed: true,
        name_template,
        ..Default::default()
    })
}

pub fn validate_text(text: &str) -> io::Result<()> {
    tokens(text).map(|_| ())
}

impl Profile {
    pub fn resolved(mut self) -> io::Result<Self> {
        if self.args.is_empty() {
            self.args = self.base_args.clone();
            if let Some(path) = &self.file {
                self.args.extend(load(path)?.args);
            }
        }
        Ok(self)
    }
    pub fn value(&self, name: &str) -> Option<&str> {
        self.args
            .windows(2)
            .rev()
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].as_str())
    }
    pub fn selector(&self) -> String {
        let mut selector = None;
        let mut audio = false;
        let mut audio_format = None;
        let mut args = self.args.iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "-f" | "--format" => selector = args.next().cloned(),
                "-x" | "--extract-audio" => audio = true,
                "--audio-format" => audio_format = args.next().cloned(),
                _ => {}
            }
        }
        if audio {
            match audio_format.as_deref() {
                Some("mp3") => {
                    let rate = self
                        .value("--audio-quality")
                        .unwrap_or("0")
                        .trim_end_matches(['K', 'k']);
                    let candidate = format!("mp3:{rate}");
                    return if crate::resolve::mp3_quality(&candidate).is_some() {
                        candidate
                    } else {
                        "mp3:0".into()
                    };
                }
                Some("opus") => return crate::resolve::OPUS.into(),
                Some("flac") => return crate::resolve::FLAC.into(),
                _ => return "ba/b".into(),
            }
        }
        selector.unwrap_or_else(|| "bv*+ba/b".into())
    }
}

// Configs use shell-style quotes, comments and backslash continuation. Never execute a shell.
fn tokens(text: &str) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let normalized = text.replace("\r\n", "\n");
    let mut it = normalized.chars().peekable();
    while let Some(c) = it.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => word.push(c),
            (Some('"'), '\\') => {
                if let Some(&n) = it.peek() {
                    if matches!(n, '"' | '\\') {
                        word.push(it.next().unwrap());
                    } else if n == '\n' {
                        it.next();
                    } else {
                        word.push(c);
                    }
                } else {
                    word.push(c);
                }
            }
            (Some(_), _) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, '#') => {
                for n in it.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, '\\') => {
                let n = it.next().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unfinished escape in yt-dlp config",
                    )
                })?;
                if n != '\n' {
                    word.push(n);
                    started = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, _) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unclosed quote in yt-dlp config",
        ));
    }
    if started {
        out.push(word);
    }
    Ok(out)
}

/// Keep preference options with known arity. Downloaders, subprocesses, additional configs,
/// URLs, output paths and mode/progress switches are controlled exclusively by Orbit.
fn preferences(tokens: &[String]) -> Vec<String> {
    const VALUES: &[&str] = &[
        "-f",
        "--format",
        "-S",
        "--format-sort",
        "--format-sort-force",
        "--merge-output-format",
        "--remux-video",
        "--recode-video",
        "--audio-format",
        "--audio-quality",
        "--js-runtimes",
        "--remote-components",
        "--sub-langs",
        "--sub-format",
        "--convert-subs",
        "--cookies",
        "--cookies-from-browser",
        "--user-agent",
        "--referer",
        "--add-header",
        "--add-headers",
        "--extractor-args",
        "--username",
        "--password",
        "--video-password",
        "--twofactor",
        "--socket-timeout",
        "--retries",
        "--fragment-retries",
        "--limit-rate",
        "--throttled-rate",
        "--sleep-requests",
        "--sponsorblock-remove",
        "--sponsorblock-mark",
        "--geo-bypass-country",
    ];
    const FLAGS: &[&str] = &[
        "-x",
        "--extract-audio",
        "--write-subs",
        "--write-auto-subs",
        "--all-subs",
        "--embed-subs",
        "--embed-thumbnail",
        "--embed-metadata",
        "--embed-info-json",
        "--no-embed-info-json",
        "--add-metadata",
        "--no-add-metadata",
        "--no-remote-components",
        "--embed-chapters",
        "--write-thumbnail",
        "--write-info-json",
        "--no-embed-subs",
        "--no-embed-thumbnail",
        "--no-embed-metadata",
        "--no-embed-chapters",
        "--no-write-subs",
        "--no-write-auto-subs",
        "--no-write-thumbnail",
        "--prefer-free-formats",
        "--no-prefer-free-formats",
        "--format-sort-force",
        "--no-format-sort-force",
        "--geo-bypass",
        "--no-geo-bypass",
    ];
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        let (name, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(a, b)| (a, Some(b)));
        if FLAGS.contains(&name) && inline.is_none() {
            out.push(name.into());
        } else if VALUES.contains(&name) {
            if let Some(value) = inline {
                out.extend([name.to_owned(), normalized_value(name, value)]);
            } else if let Some(value) = tokens.get(i + 1).filter(|v| !v.starts_with('-')) {
                out.extend([name.to_owned(), normalized_value(name, value)]);
                i += 1;
            }
        }
        i += 1;
    }
    out
}

fn normalized_value(name: &str, value: &str) -> String {
    if name == "--js-runtimes" && Path::new(value).is_absolute() {
        format!("deno:{value}")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orbit_config_profiles_are_isolated_and_restore_after_nested_work() {
        let manual = crate::preferences::Preferences::default()
            .profile(false)
            .unwrap();
        let file = Profile {
            file: Some("selected.conf".into()),
            args: vec!["-f".into(), "18".into()],
            managed: true,
            ..Default::default()
        };
        with_profile(file, || {
            assert!(active());
            assert_eq!(profile().unwrap().selector(), "18");
            with_profile(manual, || {
                assert!(!active());
                assert_eq!(
                    profile().unwrap().value("--merge-output-format"),
                    Some("mkv")
                );
            });
            assert!(active());
            assert!(!std::thread::spawn(active).join().unwrap());
        });
        assert!(!active());
    }
    #[test]
    fn orbit_config_saved_jobs_do_not_copy_configuration_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("yt-dlp.conf");
        std::fs::write(
            &file,
            "-f 18\n--password secret-value\n--embed-info-json\n--remote-components ejs:github\n",
        )
        .unwrap();
        let mut p = load(&file).unwrap();
        p.base_args = vec!["--embed-metadata".into()];
        let text = serde_json::to_string(&p).unwrap();
        assert!(!text.contains("secret-value"));
        let restored: Profile = serde_json::from_str(&text).unwrap();
        let restored = restored.resolved().unwrap();
        assert_eq!(restored.selector(), "18");
        assert!(restored.args.contains(&"--embed-info-json".into()));
        assert!(restored.args.contains(&"--embed-metadata".into()));
    }
    #[test]
    fn orbit_config_runtime_paths_use_ytdlp_runtime_syntax() {
        let args=preferences(&tokens(r#"--js-runtimes "C:\Users\Me\deno.exe" --remote-components ejs:github --embed-info-json --add-metadata"#).unwrap());
        assert_eq!(args[1], r"deno:C:\Users\Me\deno.exe");
        assert!(args.contains(&"--embed-info-json".into()));
        assert!(!args.contains(&"--exec".into()));
    }
    #[test]
    fn orbit_config_keeps_preferences_and_ignores_external_commands_and_output_controls() {
        let args=preferences(&tokens("# header\n-f 'bv*[height<=720]+ba/b'\n--downloader aria2c\n--downloader-args='aria2c:-x16'\n--exec 'bad command'\n-o 'C:/elsewhere/%(title)s'\n--config-locations nested.conf\n--embed-subs\n--sub-langs en\nhttps://unwanted.test/video\n--format-sort=codec").unwrap());
        assert_eq!(
            args,
            vec![
                "-f",
                "bv*[height<=720]+ba/b",
                "--embed-subs",
                "--sub-langs",
                "en",
                "--format-sort",
                "codec"
            ]
        );
        let profile = Profile {
            file: None,
            args,
            ..Default::default()
        };
        assert_eq!(profile.selector(), "bv*[height<=720]+ba/b");
    }
    #[test]
    fn orbit_config_quotes_windows_paths_comments_and_continuations() {
        let got=tokens("--cookies \"C:\\Users\\Me\\cookies.txt\" \\\n --user-agent 'Orbit test' # comment\n-x --audio-format mp3").unwrap();
        assert_eq!(got[1], r"C:\Users\Me\cookies.txt");
        assert_eq!(
            Profile {
                file: None,
                args: preferences(&got),
                ..Default::default()
            }
            .selector(),
            "mp3:0"
        );
        assert!(tokens("-f 'unfinished").is_err());
        assert_eq!(
            tokens("--format \\\r\n 18\r\n").unwrap(),
            vec!["--format", "18"]
        );
    }
}
