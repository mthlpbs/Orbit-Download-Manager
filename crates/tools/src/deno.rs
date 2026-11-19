//! Deno for yt-dlp, from a user selection, the system, or a verified upstream release.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Installed, chosen, github};

const REPO: &str = "denoland/deno";
const RELEASE: &str = "https://api.github.com/repos/denoland/deno/releases/latest";
pub const INSTALL_PAGE: &str = "https://docs.deno.com/runtime/getting_started/installation/";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    digest: Option<String>,
}

fn asset() -> Option<String> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return None,
    };
    Some(format!("deno-{target}.zip"))
}

pub fn available() -> bool {
    asset().is_some()
}

pub fn explicit(dir: &Path) -> Option<Installed> {
    chosen::chosen(dir, chosen::Helper::Deno).or_else(|| {
        let preferences = crate::preferences::Preferences::read_from(dir);
        let exe = PathBuf::from(preferences.deno_path.trim());
        (exe.is_absolute() && exe.is_file()).then(|| Installed {
            exe,
            version: String::new(),
        })
    })
}

pub fn reset(dir: &Path) -> Result<(), Error> {
    let mut preferences = crate::preferences::Preferences::read_from(dir);
    if !preferences.deno_path.is_empty() {
        preferences.deno_path.clear();
        preferences.save_to(dir)?;
    }
    chosen::forget(dir, chosen::Helper::Deno)
}

pub fn installed(dir: &Path) -> Option<Installed> {
    explicit(dir)
        .or_else(|| crate::system::installed("deno"))
        .or_else(|| own(dir))
}

fn own(dir: &Path) -> Option<Installed> {
    let data = std::fs::read(dir.join("deno/current.json")).ok()?;
    let current: Installed = serde_json::from_slice(&data).ok()?;
    current.exe.is_file().then_some(current)
}

pub async fn install_latest(dir: &Path, net: &mdm_engine::Options) -> Result<Installed, Error> {
    static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = INSTALL.lock().await;
    if let Some(current) = installed(dir) {
        return Ok(current);
    }
    let name = asset().ok_or_else(|| Error::Missing("Deno build for this system".into()))?;
    let client = crate::client(net, RELEASE)?;
    let release: Release = serde_json::from_slice(
        &client
            .get(RELEASE)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?,
    )
    .map_err(|e| Error::BadRelease(e.to_string()))?;
    let version = release.tag_name.strip_prefix('v').filter(|v| {
        let parts: Vec<_> = v.split('.').collect();
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
    });
    let version = version.ok_or_else(|| Error::BadRelease(release.tag_name.clone()))?;
    let digest = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.digest.as_deref())
        .and_then(|d| d.strip_prefix("sha256:"))
        .and_then(|d| hex::decode(d).ok())
        .and_then(|d| <[u8; 32]>::try_from(d).ok())
        .ok_or_else(|| Error::Missing(format!("SHA-256 digest for {name}")))?;
    let url = github::download_url(github::WEB, REPO, &release.tag_name, &name);
    let root = dir.join("deno");
    std::fs::create_dir_all(&root)?;
    let archive = root.join(format!("deno-{}.zip", hex::encode(&digest[..8])));
    crate::fetch_verified(&url, &archive, digest, net, None).await?;
    let exe = unpack(&archive, &root.join(version), &url);
    let _ = std::fs::remove_file(archive);
    crate::make_current(
        &root,
        Installed {
            version: version.to_owned(),
            exe: exe?,
        },
    )
}

fn unpack(archive: &Path, folder: &Path, from: &str) -> Result<PathBuf, Error> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)
        .map_err(|e| Error::BadRelease(e.to_string()))?;
    let name = crate::system::program_name("deno");
    let mut entry = zip
        .by_name(&name)
        .map_err(|e| Error::BadRelease(e.to_string()))?;
    if !entry.is_file() || entry.enclosed_name().as_deref() != Some(Path::new(&name)) {
        return Err(Error::Missing(name));
    }
    std::fs::create_dir_all(folder)?;
    let exe = folder.join(name);
    let temp = exe.with_extension("part");
    let mut out = std::fs::File::create(&temp)?;
    std::io::copy(&mut entry, &mut out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        out.set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    drop(out);
    std::fs::rename(temp, &exe)?;
    crate::mark_downloaded(&exe, from);
    Ok(exe)
}
