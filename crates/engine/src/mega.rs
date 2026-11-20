//! Public MEGA links: protocol and crypto adapted from MegaBasterd (GPL-3.0).
//! Reference: tonikelope/megabasterd, MegaAPI.java, CryptTools.java and
//! Download.verifyFileCBCMAC. All API and storage requests use Orbit's network settings.
use crate::{Error, Options};
use aes::Aes128;
use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};
use url::Url;

const API: &str = "https://g.api.mega.co.nz/cs";

/// A stable cache slot per reserved final destination; link refreshes keep the same data.
pub fn cache_base(_url: &Url, dest: &Path, configured: Option<&Path>) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let root = configured
        .map(Path::to_owned)
        .or_else(|| {
            std::env::var_os("ODM_DATA_DIR")
                .or_else(|| std::env::var_os("MDM_DATA_DIR"))
                .map(std::path::PathBuf::from)
                .map(|d| d.join("cache/downloads"))
        })
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .map(|d| d.join("OrbitDownloadManager/cache/downloads"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("OrbitDownloadManager/download-cache"));
    let mut hash = Sha256::new();
    hash.update(b"Orbit-MEGA-cache-v1");
    hash.update([0]);
    hash.update(dest.to_string_lossy().as_bytes());
    root.join(hex::encode(hash.finalize()))
}
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub enum Link {
    File {
        id: String,
        key: [u8; 32],
        folder: Option<String>,
    },
    Folder {
        id: String,
        key: [u8; 16],
        selected: Option<String>,
    },
}

pub fn is_mega(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("mega.nz" | "www.mega.nz" | "mega.co.nz" | "www.mega.co.nz")
    )
}

fn bad(message: impl Into<String>) -> Error {
    Error::Mega(message.into())
}
fn decode(text: &str) -> Result<Vec<u8>, Error> {
    URL_SAFE_NO_PAD
        .decode(text.trim_end_matches('='))
        .map_err(|_| bad("Invalid MEGA decryption key"))
}
fn key<const N: usize>(text: &str) -> Result<[u8; N], Error> {
    decode(text)?
        .try_into()
        .map_err(|_| bad("The MEGA link is missing its complete decryption key"))
}
fn handle(text: &str) -> Result<String, Error> {
    if text.len() != 8
        || !text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(bad("Invalid MEGA file or folder identifier"));
    }
    Ok(text.into())
}

pub fn parse(url: &Url) -> Result<Link, Error> {
    if !is_mega(url) || !matches!(url.scheme(), "http" | "https") {
        return Err(bad("Not a MEGA public link"));
    }
    let fragment = url.fragment().unwrap_or_default();
    let parts: Vec<_> = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    match parts.as_slice() {
        ["file", id] => {
            return Ok(Link::File {
                id: handle(id)?,
                key: key(fragment)?,
                folder: None,
            });
        }
        ["folder", id] => {
            let (secret, selection) = fragment.split_once('/').unwrap_or((fragment, ""));
            let selected = selection
                .strip_prefix("file/")
                .or_else(|| selection.strip_prefix("folder/"))
                .map(handle)
                .transpose()?;
            return Ok(Link::Folder {
                id: handle(id)?,
                key: key(secret)?,
                selected,
            });
        }
        _ => {}
    }
    if let Some(rest) = fragment.strip_prefix("N!") {
        let (file, folder) = rest
            .split_once("###n=")
            .ok_or_else(|| bad("Incomplete MEGA folder-file link"))?;
        let (id, secret) = file
            .split_once('!')
            .ok_or_else(|| bad("Incomplete MEGA file link"))?;
        return Ok(Link::File {
            id: handle(id)?,
            key: key(secret)?,
            folder: Some(handle(folder)?),
        });
    }
    if let Some(rest) = fragment.strip_prefix("F!") {
        let (id, secret) = rest
            .split_once('!')
            .ok_or_else(|| bad("Incomplete MEGA folder link"))?;
        let (secret, selected) = secret.split_once('!').unwrap_or((secret, ""));
        return Ok(Link::Folder {
            id: handle(id)?,
            key: key(secret)?,
            selected: (!selected.is_empty())
                .then(|| handle(selected))
                .transpose()?,
        });
    }
    if let Some(rest) = fragment.strip_prefix('!') {
        let (id, secret) = rest
            .split_once('!')
            .ok_or_else(|| bad("Incomplete MEGA file link"))?;
        return Ok(Link::File {
            id: handle(id)?,
            key: key(secret)?,
            folder: None,
        });
    }
    Err(bad(
        "Paste a public MEGA file or folder link including its # decryption key",
    ))
}

pub fn is_folder(url: &Url) -> bool {
    matches!(parse(url), Ok(Link::Folder { .. }))
}

fn api_error(code: i64) -> Error {
    bad(match code {
        -3 | -4 | -18 => "MEGA is busy. Try again later.",
        -9 => "The MEGA file or folder was removed or the link is invalid.",
        -11 => "MEGA denied access to this file or folder.",
        -14 => "The MEGA decryption key is incorrect.",
        -17 => {
            "MEGA's transfer quota has been reached. Wait and resume later, or configure your proxy in Options → Proxy / Socks."
        }
        _ => return bad(format!("MEGA API error {code}")),
    })
}

async fn api_at(
    endpoint: &Url,
    command: Value,
    folder: Option<&str>,
    opts: &Options,
) -> Result<Value, Error> {
    let mut url = endpoint.clone();
    url.query_pairs_mut()
        .append_pair("id", &SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string())
        .append_pair("lang", "en");
    if let Some(folder) = folder {
        url.query_pairs_mut().append_pair("n", folder);
    }
    // No browser cookies or authorization headers are forwarded to the API/storage server.
    let client = crate::http_client(opts, &url)?;
    let mut response = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&vec![command]).map_err(|_| bad("Invalid MEGA API request"))?)
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > 32 << 20 {
            return Err(bad("MEGA returned an excessively large response"));
        }
        body.extend_from_slice(&chunk);
    }
    let result: Value =
        serde_json::from_slice(&body).map_err(|_| bad("Invalid response from MEGA"))?;
    let result = result.as_array().and_then(|a| a.first()).unwrap_or(&result);
    if let Some(code) = result.as_i64().filter(|n| *n < 0) {
        return Err(api_error(code));
    }
    if let Some(code) = result.get("e").and_then(Value::as_i64).filter(|n| *n < 0) {
        return Err(api_error(code));
    }
    Ok(result.clone())
}

#[derive(Clone)]
pub struct File {
    pub name: String,
    pub size: u64,
    pub url: Url,
    pub key: [u8; 32],
    pub modified: Option<SystemTime>,
}

pub async fn resolve(url: &Url, opts: &Options) -> Result<File, Error> {
    resolve_at(url, opts, &Url::parse(API).expect("static MEGA API URL")).await
}

pub(crate) async fn resolve_at(url: &Url, opts: &Options, endpoint: &Url) -> Result<File, Error> {
    let link = match parse(url)? {
        Link::Folder { .. } => {
            let files = folder_at(url, opts, endpoint).await?;
            let Link::Folder {
                selected: Some(selected),
                ..
            } = parse(url)?
            else {
                return Err(bad(
                    "This is a MEGA folder. Open it in Link grabber to select its files.",
                ));
            };
            let found = files
                .into_iter()
                .find(|f| matches!(parse(f), Ok(Link::File { id, .. }) if id == selected))
                .ok_or_else(|| bad("The selected MEGA file is not in this folder"))?;
            parse(&found)?
        }
        file => file,
    };
    let Link::File { id, key, folder } = link else {
        unreachable!()
    };
    let mut command = json!({"a":"g","g":1,"ssl":2});
    command[if folder.is_some() { "n" } else { "p" }] = id.into();
    let response = api_at(endpoint, command, folder.as_deref(), opts).await?;
    let attributes = attributes(
        response["at"]
            .as_str()
            .ok_or_else(|| bad("MEGA returned no file attributes"))?,
        &aes_key(&key),
    )?;
    let name = attributes["n"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("MEGA returned no file name"))?;
    let direct = response["g"]
        .as_str()
        .ok_or_else(|| bad("MEGA returned no download URL"))?;
    let direct = Url::parse(direct).map_err(|_| bad("MEGA returned an invalid download URL"))?;
    if !matches!(direct.scheme(), "http" | "https")
        || !direct.username().is_empty()
        || direct.password().is_some()
    {
        return Err(bad("MEGA returned an unsupported download URL"));
    }
    // Production links must use authenticated HTTPS. Local endpoints are only used by tests.
    if endpoint.scheme() == "https" && direct.scheme() != "https" {
        return Err(bad("MEGA returned an insecure download URL"));
    }
    let modified = attributes["mtime"]
        .as_u64()
        .or_else(|| response["ts"].as_u64())
        .and_then(|s| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(s)));
    Ok(File {
        name: crate::naming::sanitize(name),
        size: response["s"]
            .as_u64()
            .ok_or_else(|| bad("MEGA returned no file size"))?,
        url: direct,
        key,
        modified,
    })
}

pub async fn folder(url: &Url, opts: &Options) -> Result<Vec<Url>, Error> {
    folder_at(url, opts, &Url::parse(API).expect("static MEGA API URL")).await
}

pub(crate) async fn folder_at(
    url: &Url,
    opts: &Options,
    endpoint: &Url,
) -> Result<Vec<Url>, Error> {
    let Link::Folder { id, key, selected } = parse(url)? else {
        return Err(bad("Not a MEGA folder link"));
    };
    let response = api_at(endpoint, json!({"a":"f","c":1,"r":1}), Some(&id), opts).await?;
    let nodes = response["f"]
        .as_array()
        .ok_or_else(|| bad("MEGA returned no folder contents"))?;
    let selected_file = selected.as_ref().is_some_and(|s| {
        nodes
            .iter()
            .any(|n| n["h"].as_str() == Some(s) && n["t"] == 0)
    });
    let mut out = Vec::new();
    for node in nodes.iter().filter(|n| n["t"] == 0) {
        let Some(node_id) = node["h"].as_str() else {
            continue;
        };
        if let Some(selected) = &selected {
            if selected_file && node_id != selected {
                continue;
            }
            if !selected_file {
                let mut parent = node["p"].as_str();
                let mut included = node_id == selected;
                for _ in 0..nodes.len().min(256) {
                    let Some(p) = parent else { break };
                    if p == selected {
                        included = true;
                        break;
                    }
                    parent = nodes
                        .iter()
                        .find(|n| n["h"].as_str() == Some(p))
                        .and_then(|n| n["p"].as_str());
                }
                if !included {
                    continue;
                }
            }
        }
        let node_key = node["k"]
            .as_str()
            .unwrap_or_default()
            .split('/')
            .find_map(|candidate| {
                let (_, encoded) = candidate.split_once(':')?;
                let bytes = decode(encoded).ok()?;
                let decrypted = decrypt_ecb(&bytes, &key).ok()?;
                let file_key: [u8; 32] = decrypted.try_into().ok()?;
                attributes(node["a"].as_str()?, &aes_key(&file_key)).ok()?;
                Some(file_key)
            })
            .ok_or_else(|| {
                bad("A MEGA folder file could not be decrypted. Check the folder key.")
            })?;
        handle(node_id)?;
        out.push(
            Url::parse(&format!(
                "https://mega.nz/#N!{node_id}!{}###n={id}",
                URL_SAFE_NO_PAD.encode(node_key)
            ))
            .map_err(|_| bad("Invalid MEGA folder file"))?,
        );
    }
    Ok(out)
}

pub fn aes_key(key: &[u8; 32]) -> [u8; 16] {
    std::array::from_fn(|i| key[i] ^ key[i + 16])
}
fn decrypt_ecb(data: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, Error> {
    if data.is_empty() || !data.len().is_multiple_of(16) {
        return Err(bad("Invalid encrypted MEGA data"));
    }
    let cipher = Aes128::new(&(*key).into());
    let mut out = data.to_vec();
    for block in out.chunks_exact_mut(16) {
        let mut b = Array::try_from(&*block).expect("16-byte block");
        cipher.decrypt_block(&mut b);
        block.copy_from_slice(&b);
    }
    Ok(out)
}
fn attributes(data: &str, key: &[u8; 16]) -> Result<Value, Error> {
    let data = decode(data)?;
    let mut plain = decrypt_ecb(&data, key)?;
    let mut previous = [0u8; 16];
    for (encrypted, decrypted) in data.chunks_exact(16).zip(plain.chunks_exact_mut(16)) {
        for i in 0..16 {
            decrypted[i] ^= previous[i];
        }
        previous.copy_from_slice(encrypted);
    }
    let plain = plain
        .strip_prefix(b"MEGA")
        .ok_or_else(|| bad("The MEGA decryption key is incorrect"))?;
    let end = plain.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    serde_json::from_slice(&plain[..end])
        .map_err(|_| bad("Invalid MEGA file attributes or decryption key"))
}

/// AES-CTR at any byte offset, including an unaligned resumed HTTP chunk.
pub fn decrypt_at(data: &mut [u8], key: &[u8; 32], offset: u64) {
    let cipher = Aes128::new(&aes_key(key).into());
    let mut index = 0;
    let mut at = offset;
    while index < data.len() {
        let mut counter = [0u8; 16];
        counter[..8].copy_from_slice(&key[16..24]);
        counter[8..].copy_from_slice(&(at / 16).to_be_bytes());
        let mut stream = Array::from(counter);
        cipher.encrypt_block(&mut stream);
        let skip = (at % 16) as usize;
        let take = (16 - skip).min(data.len() - index);
        for i in 0..take {
            data[index + i] ^= stream[skip + i];
        }
        index += take;
        at += take as u64;
    }
}

/// Verify MEGA's mandatory chunk CBC-MAC, as MegaBasterd's verifyFileCBCMAC does.
pub fn verify(
    path: &Path,
    key: &[u8; 32],
    size: u64,
    cancel: &tokio_util::sync::CancellationToken,
    progress: &AtomicU64,
) -> Result<(), Error> {
    let mut file = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
    let mac = mac(&mut file, key, size, cancel, progress)?;
    let mut expected = [0u8; 8];
    expected.copy_from_slice(&key[24..32]);
    if mac != expected {
        return Err(Error::ChecksumMismatch("MEGA CBC-MAC"));
    }
    Ok(())
}

fn mac(
    reader: &mut impl Read,
    key: &[u8; 32],
    size: u64,
    cancel: &tokio_util::sync::CancellationToken,
    progress: &AtomicU64,
) -> Result<[u8; 8], Error> {
    let cipher = Aes128::new(&aes_key(key).into());
    let mut file_mac = [0u8; 16];
    let mut done = 0;
    let mut chunk = 1u64;
    while done < size {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut chunk_mac = [0u8; 16];
        chunk_mac[..8].copy_from_slice(&key[16..24]);
        chunk_mac[8..].copy_from_slice(&key[16..24]);
        let length = (chunk.min(8) * 131072).min(size - done);
        let mut left = length;
        while left > 0 {
            let mut block = [0u8; 16];
            let n = left.min(16) as usize;
            reader.read_exact(&mut block[..n])?;
            for i in 0..16 {
                chunk_mac[i] ^= block[i];
            }
            let mut block = Array::from(chunk_mac);
            cipher.encrypt_block(&mut block);
            chunk_mac.copy_from_slice(&block);
            left -= n as u64;
        }
        for i in 0..16 {
            file_mac[i] ^= chunk_mac[i];
        }
        let mut block = Array::from(file_mac);
        cipher.encrypt_block(&mut block);
        file_mac.copy_from_slice(&block);
        done += length;
        progress.store(done, Ordering::Relaxed);
        chunk += 1;
    }
    Ok(std::array::from_fn(|i| {
        if i < 4 {
            file_mac[i] ^ file_mac[i + 4]
        } else {
            file_mac[i + 4] ^ file_mac[i + 8]
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_current_legacy_and_folder_file_links_and_rejects_missing_keys() {
        let encoded = URL_SAFE_NO_PAD.encode([7u8; 32]);
        for s in [
            format!("https://mega.nz/file/Abcd1234#{encoded}"),
            format!("https://mega.nz/#!Abcd1234!{encoded}"),
            format!("https://mega.nz/#N!Abcd1234!{encoded}###n=Folder12"),
        ] {
            assert!(matches!(
                parse(&Url::parse(&s).unwrap()),
                Ok(Link::File { .. })
            ));
        }
        assert!(parse(&Url::parse("https://mega.nz/file/Abcd1234").unwrap()).is_err());
        assert!(!is_mega(
            &Url::parse("https://mega.nz.evil.test/file/a").unwrap()
        ));
        let folder = format!(
            "https://mega.nz/folder/Folder12#{}",
            URL_SAFE_NO_PAD.encode([9u8; 16])
        );
        assert!(is_folder(&Url::parse(&folder).unwrap()));
    }
    #[test]
    fn ctr_matches_nist_and_supports_unaligned_resume() {
        // NIST SP 800-38A F.5.1 validates AES; MEGA uses an eight-byte nonce and counter.
        let aes: [u8; 16] = hex::decode("2b7e151628aed2a6abf7158809cf4f3c")
            .unwrap()
            .try_into()
            .unwrap();
        let mut key = [0u8; 32];
        key[16..24].copy_from_slice(&hex::decode("f0f1f2f3f4f5f6f7").unwrap());
        for i in 0..16 {
            key[i] = aes[i] ^ key[i + 16];
        }
        let input = hex::decode("6bc1bee22e409f96e93d7e117393172a").unwrap();
        let offset =
            u64::from_be_bytes(hex::decode("f8f9fafbfcfdfeff").unwrap().try_into().unwrap());
        // The NIST counter is larger than a byte offset can encode. Direct AES validates the
        // primitive, and the MEGA CTR offset is compared with that independently computed block.
        let mut counter = Array::try_from(
            hex::decode("f0f1f2f3f4f5f6f70000000000000000")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let cipher = Aes128::new(&aes.into());
        cipher.encrypt_block(&mut counter);
        let expected: Vec<_> = input
            .iter()
            .zip(counter.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        let mut got = input.clone();
        decrypt_at(&mut got, &key, 0);
        assert_eq!(got, expected);
        let mut tail = input[5..].to_vec();
        decrypt_at(&mut tail, &key, 5);
        assert_eq!(tail, expected[5..]);
        let mut block = Array::try_from(
            hex::decode("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        cipher.encrypt_block(&mut block);
        assert_eq!(
            input
                .iter()
                .zip(block.iter())
                .map(|(a, b)| a ^ b)
                .collect::<Vec<_>>(),
            hex::decode("874d6191b620e3261bef6864990db6ce").unwrap()
        );
        assert!(offset > 0);
    }
    #[test]
    fn cbc_mac_catches_corruption_across_chunk_boundaries() {
        let body: Vec<u8> = (0..(131072 * 12 + 7)).map(|n| (n % 251) as u8).collect();
        let mut key = [3u8; 32];
        let original_aes = aes_key(&key);
        let token = tokio_util::sync::CancellationToken::new();
        let progress = AtomicU64::new(0);
        let expected = mac(&mut &body[..], &key, body.len() as u64, &token, &progress).unwrap();
        key[24..].copy_from_slice(&expected);
        for i in 0..16 {
            key[i] = original_aes[i] ^ key[i + 16];
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), &body).unwrap();
        verify(file.path(), &key, body.len() as u64, &token, &progress).unwrap();
        let mut changed = body;
        changed[131073] ^= 1;
        std::fs::write(file.path(), &changed).unwrap();
        assert!(matches!(
            verify(file.path(), &key, changed.len() as u64, &token, &progress),
            Err(Error::ChecksumMismatch(_))
        ));
    }
}
