//! MEGA transfer data lives in Orbit's cache; only a verified finished file reaches Save to.
use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

fn base(url: &Url, dest: &Path, opts: &Options) -> PathBuf {
    crate::mega::cache_base(url, dest, opts.mega_cache.as_deref())
}

pub(super) async fn resume_or_fresh(
    req: Request,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    if let Destination::File(dest) | Destination::Free(dest) = &req.dest {
        let cached = state_path(&base(&req.url, dest, &opts));
        let legacy = state_path(dest);
        let state = if cached.exists() {
            Some(cached)
        } else if legacy.exists() {
            Some(legacy)
        } else {
            None
        };
        if let Some(state) = state {
            let saved = ResumeState::load(&state).map_err(Error::BadState)?;
            if saved.url != req.url || saved.dest != *dest {
                return Err(Error::BadState(
                    "MEGA cache belongs to another download".into(),
                ));
            }
            return resume(saved, state, opts, shared, cancel).await;
        }
    }
    fresh(req, opts, shared, cancel).await
}

pub(super) async fn fresh(
    req: Request,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    let meta = cancellable(&cancel, crate::mega::resolve(&req.url, &opts)).await?;
    let dest = match &req.dest {
        Destination::File(p) => p.clone(),
        Destination::Free(p) => {
            if opts.replace_existing {
                p.clone()
            } else {
                naming::unique_path(
                    p.parent().unwrap_or(Path::new(".")),
                    p.file_name()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or("download"),
                )
            }
        }
        Destination::Dir(dir) => naming::unique_path(
            dir,
            &naming::fit_name(dir, &meta.name)
                .ok_or_else(|| Error::PathTooLong(dir.join(&meta.name)))?,
        ),
    };
    if !naming::path_fits(&dest) {
        return Err(Error::PathTooLong(dest));
    }
    let folder = dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder)
        .map_err(|_| Error::DestinationUnavailable(folder.to_owned()))?;
    let reservation = naming::reserve(&dest).ok_or_else(|| Error::InUse(dest.clone()))?;
    let cache = base(&req.url, &dest, &opts);
    std::fs::create_dir_all(cache.parent().expect("cache has a parent"))?;
    if let Some(volume) = disk::probe(cache.parent().unwrap().to_owned()).await {
        disk::check(&volume, meta.size)?;
    }
    if let Some(volume) = disk::probe(folder.to_owned()).await {
        disk::check(&volume, meta.size)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(naming::part_path(&cache))?;
    if meta.size >= sparse::MIN_SIZE {
        sparse::set(&file, true);
    }
    file.set_len(meta.size)?;
    if let Some(hook) = &opts.on_file_created {
        (hook.0)(&naming::part_path(&cache));
    }
    let mut saved = ResumeState::new(
        req.url.clone(),
        req.url,
        dest,
        meta.size,
        Validators::default(),
    );
    saved.checksum = opts.checksum.clone();
    receive(meta, saved, file, reservation, opts, shared, cancel, 0).await
}

pub(super) async fn resume(
    saved: ResumeState,
    state_file: PathBuf,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    let cache = base(&saved.url, &saved.dest, &opts);
    let cached_state = state_path(&cache);
    if state_file != cached_state && state_file != state_path(&saved.dest) {
        return Err(Error::BadState(
            "MEGA state does not belong to this download".into(),
        ));
    }
    after_settling(&Destination::File(saved.dest.clone()), &cancel).await?;
    let saved = ResumeState::load(&state_file).map_err(Error::BadState)?;
    if state_file != cached_state {
        std::fs::create_dir_all(cache.parent().unwrap())?;
        let legacy = naming::part_path(&saved.dest);
        let target = naming::part_path(&cache);
        if legacy.exists() && !target.exists() {
            match std::fs::rename(&legacy, &target) {
                Ok(()) => {}
                Err(_) => {
                    std::fs::copy(&legacy, &target)?;
                    std::fs::remove_file(&legacy)?;
                }
            }
        }
        saved.write(&cached_state)?;
        std::fs::remove_file(&state_file)?;
    }
    let meta = cancellable(&cancel, crate::mega::resolve(&saved.url, &opts)).await?;
    if meta.size != saved.size {
        return Err(Error::FileChanged);
    }
    let reservation =
        naming::reserve(&saved.dest).ok_or_else(|| Error::InUse(saved.dest.clone()))?;
    let part = naming::part_path(&cache);
    if !part.exists() && std::fs::metadata(&saved.dest).is_ok_and(|m| m.len() == saved.size) {
        let _ = std::fs::remove_file(&cached_state);
        return Ok(Outcome {
            path: saved.dest,
            size: saved.size,
        });
    }
    let cursor = match saved.segments.as_slice() {
        [] => 0,
        [s] if s.start == 0 && s.end == saved.size && s.cursor <= s.end => s.cursor,
        _ => return Err(Error::BadState("Invalid MEGA resume positions".into())),
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&part)?;
    if file.metadata()?.len() < cursor {
        return Err(Error::BadState(
            "The MEGA cache file is shorter than its saved position".into(),
        ));
    }
    receive(meta, saved, file, reservation, opts, shared, cancel, cursor).await
}

#[allow(clippy::too_many_arguments)]
async fn receive(
    meta: crate::mega::File,
    mut saved: ResumeState,
    mut file: File,
    _reservation: naming::Reservation,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
    mut cursor: u64,
) -> Result<Outcome, Error> {
    let size = meta.size;
    let dest = saved.dest.clone();
    let cache = base(&saved.url, &dest, &opts);
    let state_file = state_path(&cache);
    let segment = |cursor| {
        vec![crate::planner::SavedSegment {
            start: 0,
            cursor,
            end: size,
        }]
    };
    saved.save(&state_file, segment(cursor), &file)?;
    downloading(&shared, &dest, Some(size), RANGES_YES);
    shared.limiter.set(opts.speed_limit);
    shared.event("MEGA: downloading to Orbit's cache through the configured connection");
    let started = std::time::Instant::now();
    let initial = cursor;
    let mut last_save = std::time::Instant::now();
    let client = http_client(&opts, &meta.url)?;
    let result:Result<(),Error>=async {
        let mut retries=0;
        while cursor<size {
            if cancel.is_cancelled(){return Err(Error::Cancelled);}
            let end=(cursor.saturating_add(4<<20)).min(size)-1;
            let target=Url::parse(&format!("{}/{cursor}-{end}",meta.url.as_str().trim_end_matches('/'))).map_err(|_|Error::Mega("Invalid MEGA storage URL".into()))?;
            shared.conn(0,|c|c.status=ConnStatus::Connecting);
            let attempt:Result<(),Error>=async {
                let mut response=cancellable(&cancel,async{Ok(client.get(target).send().await?)}).await?;
                if response.status().as_u16()==509{return Err(Error::Mega("MEGA's transfer quota has been reached. Wait and resume later, or configure a proxy in Settings → Proxy / Socks.".into()));}
                if !response.status().is_success(){return Err(Error::Status(response.status()));}
                if opts.server_time && meta.modified.is_none() && saved.validators.last_modified.is_none(){saved.validators.last_modified=response.headers().get(header::LAST_MODIFIED).and_then(|h|h.to_str().ok()).and_then(|h|httpdate::parse_http_date(h).ok()).map(httpdate::fmt_http_date);}
                let expected=end+1;
                while let Some(chunk)=cancellable(&cancel,async{Ok(response.chunk().await?)}).await? {
                    if chunk.len()as u64>expected-cursor{return Err(Error::BadRange("MEGA returned too much data".into()));}
                    let mut bytes=chunk.to_vec();crate::mega::decrypt_at(&mut bytes,&meta.key,cursor);
                    cancellable(&cancel,async{shared.limiter.acquire(bytes.len()as u64).await;Ok(())}).await?;
                    if let Some(global)=&opts.global{cancellable(&cancel,async{global.acquire(bytes.len()as u64).await;Ok(())}).await?;}
                    file.seek(SeekFrom::Start(cursor))?;file.write_all(&bytes)?;cursor+=bytes.len()as u64;
                    {let mut view=shared.view.lock().unwrap();view.downloaded=cursor;view.segments=vec![SegmentView{start:0,cursor,end:size}];}
                    shared.conn(0,|c|{c.status=ConnStatus::Receiving;c.downloaded=cursor;c.speed=((cursor-initial)as f64/started.elapsed().as_secs_f64().max(0.001))as u64;});
                    if last_save.elapsed()>=opts.save_interval{saved.save(&state_file,segment(cursor),&file)?;last_save=std::time::Instant::now();}
                }
                if cursor!=expected{return Err(Error::ShortBody);}Ok(())
            }.await;
            match attempt {
                Ok(())=>retries=0,
                Err(e) if !matches!(e,Error::Cancelled|Error::Mega(_)|Error::Io(_)|Error::BadRange(_))&&retries<opts.max_retries=>{retries+=1;shared.conn(0,|c|c.status=ConnStatus::Retrying(e.clone()));cancellable(&cancel,async{tokio::time::sleep(Duration::from_secs((1u64<<retries.min(5)).min(30))).await;Ok(())}).await?;},
                Err(e)=>return Err(e)
            }
        }Ok(())
    }.await;
    if matches!(result, Err(Error::Cancelled)) {
        shared.stopping(&dest);
    }
    saved.save(&state_file, segment(cursor), &file)?;
    result?;
    shared.every_byte_in();
    shared.set_state(State::Verifying);
    let part = naming::part_path(&cache);
    let key = meta.key;
    let token = cancel.clone();
    let progress = shared.checked.clone();
    let verify_path = part.clone();
    tokio::task::spawn_blocking(move || {
        crate::mega::verify(&verify_path, &key, size, &token, &progress)
    })
    .await
    .expect("MEGA verifier panicked")?;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(hash) = opts.checksum.as_ref().or(saved.checksum.as_ref()).cloned() {
        shared.checked.store(0, Ordering::Relaxed);
        let progress = shared.checked.clone();
        let path = part.clone();
        let algo = hash.algo;
        if !tokio::task::spawn_blocking(move || {
            crate::verify::matches_counted(&path, &hash, &progress)
        })
        .await
        .expect("checksum verifier panicked")?
        {
            return Err(Error::ChecksumMismatch(algo.name()));
        }
    }
    shared.event("MEGA CBC-MAC integrity verified");
    shared.set_state(State::Completing);
    file.sync_all()?;
    drop(file);
    let stamp = opts
        .server_time
        .then_some(meta.modified.or_else(|| {
            saved
                .validators
                .last_modified
                .as_deref()
                .and_then(|s| httpdate::parse_http_date(s).ok())
        }))
        .flatten();
    let replace = opts.replace_existing;
    let hook = opts.on_file_created.clone();
    let token = cancel.clone();
    let out = tokio::task::spawn_blocking(move || {
        finish_cached(&part, &dest, size, stamp, replace, hook, token)
    })
    .await
    .expect("MEGA final move panicked")?;
    std::fs::remove_file(state_file)?;
    Ok(out)
}

fn finish_cached(
    part: &Path,
    dest: &Path,
    size: u64,
    stamp: Option<std::time::SystemTime>,
    replace: bool,
    hook: Option<FileHook>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    let dir = dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let actual = if matches!(final_place(dest, replace), Final::Numbered) {
        naming::unique_path(
            dir,
            dest.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("download"),
        )
    } else {
        dest.to_owned()
    };
    if let Some(stamp) = stamp {
        let f = std::fs::OpenOptions::new().write(true).open(part)?;
        let times = std::fs::FileTimes::new()
            .set_modified(stamp)
            .set_accessed(stamp);
        #[cfg(windows)]
        let times = {
            use std::os::windows::fs::FileTimesExt;
            times.set_created(stamp)
        };
        f.set_times(times)?;
    }
    // Fast atomic move on the same volume. Cross-volume moves copy the complete verified
    // file to a short-lived staging file, never stream network bytes into the save folder.
    if std::fs::rename(part, &actual).is_ok() {
        return Ok(Outcome { path: actual, size });
    }
    let (stage, mut output) = (0..100)
        .find_map(|n| {
            let p = dir.join(format!(".orbit-complete-{}-{n}.tmp", std::process::id()));
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
                .ok()
                .map(|f| (p, f))
        })
        .ok_or_else(|| io::Error::other("Cannot create final-file staging file"))?;
    let result = (|| -> Result<(), Error> {
        let mut input = std::fs::File::open(part)?;
        let mut buffer = vec![0; 1 << 20];
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
        if let Some(stamp) = stamp {
            let times = std::fs::FileTimes::new()
                .set_modified(stamp)
                .set_accessed(stamp);
            #[cfg(windows)]
            let times = {
                use std::os::windows::fs::FileTimesExt;
                times.set_created(stamp)
            };
            output.set_times(times)?;
        }
        Ok(())
    })();
    drop(output);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&stage);
        return Err(e);
    }
    if let Some(hook) = hook {
        (hook.0)(&stage);
    }
    if let Err(e) = std::fs::rename(&stage, &actual) {
        let _ = std::fs::remove_file(&stage);
        return Err(e.into());
    }
    std::fs::remove_file(part)?;
    Ok(Outcome { path: actual, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use std::time::SystemTime;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Generated independently with Python cryptography (AES-CBC/CTR), not Orbit's crypto.
    const KEY: &str = "10101010101010107f2d3be97d81657e1011121314151617772431e2718c6b71";
    const ATTR: &str =
        "mC6lmed0meK_Hm0AfC4fE1Dal6D8AOn71PvIkdZsLK3s8cvTk9l64Hg2gJlhIuYQa8q1gOBFwI6hMBMHFxr4_w";
    const PREFIX: &str = "8de7dac3d95eca9fd74f30c1ecf8247a8f25d1b3fd2d11a8a7b458d16a085434381abad4fc22edcfdb743e5361fad8b5d0799d5c9db89392e0fadda310eefde7";

    async fn proxy(
        body: Vec<u8>,
        corrupt: bool,
        quota: bool,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let body = body.clone();
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    loop {
                        let mut buf = [0u8; 4096];
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        request.extend_from_slice(&buf[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let head = String::from_utf8_lossy(&request).to_string();
                    assert!(
                        head.to_lowercase().contains(
                            "proxy-authorization: basic dGVzdDpwYXNz"
                                .to_lowercase()
                                .as_str()
                        )
                    );
                    let target = head
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    seen.lock().unwrap().push(target.to_owned());
                    let (status, mut response) = if target.contains("folder-api.test") {
                        ("200 OK", serde_json::to_vec(&serde_json::json!([{ "f": [
                            {"h":"Folder12","t":1},
                            {"h":"SubFold1","t":1,"p":"Folder12"},
                            {"h":"Abcd1234","t":0,"p":"Folder12","a":ATTR,"k":"Other123:AAAA/Folder12:3p3L_uYa-UeLZCSK_LWAXimYm_lRN0lRUW_GKBw7FiM"},
                            {"h":"File2222","t":0,"p":"SubFold1","a":ATTR,"k":"Folder12:3p3L_uYa-UeLZCSK_LWAXimYm_lRN0lRUW_GKBw7FiM"}
                        ]}])).unwrap())
                    } else if target.contains("mega-api.test") {
                        ("200 OK",serde_json::to_vec(&serde_json::json!([{"s":body.len(),"at":ATTR,"g":"http://mega-storage.test/object"}])).unwrap())
                    } else if quota {
                        ("509 Bandwidth Limit Exceeded", Vec::new())
                    } else {
                        let range = target.rsplit('/').next().unwrap();
                        let (a, b) = range.split_once('-').unwrap();
                        let a: usize = a.parse().unwrap();
                        let b: usize = b.parse().unwrap();
                        let mut bytes = body[a..=b].to_vec();
                        let key = hex::decode(KEY).unwrap().try_into().unwrap();
                        crate::mega::decrypt_at(&mut bytes, &key, a as u64);
                        if a == 0 {
                            assert_eq!(&bytes[..64], hex::decode(PREFIX).unwrap());
                        }
                        ("200 OK", bytes)
                    };
                    if corrupt && !target.contains("mega-api.test") && !response.is_empty() {
                        response[0] ^= 1;
                    }
                    let header = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                        response.len()
                    );
                    socket.write_all(header.as_bytes()).await.unwrap();
                    socket.write_all(&response).await.unwrap();
                });
            }
        });
        (address, requests, task)
    }

    async fn transfer(corrupt: bool, quota: bool, cursor: u64) -> Result<(), Error> {
        let body: Vec<u8> = (0..((5 << 20) + 7)).map(|n| (n % 251) as u8).collect();
        let (proxy, seen, server) = proxy(body.clone(), corrupt, quota).await;
        let folder = tempfile::tempdir().unwrap();
        let opts = Options {
            mega_cache: Some(folder.path().join("cache")),
            proxy: Proxy::Manual {
                url: proxy,
                login: Some(("test".into(), "pass".into())),
                http: true,
                https: true,
            },
            server_time: true,
            ..Default::default()
        };
        let key = hex::decode(KEY).unwrap();
        let url = Url::parse(&format!(
            "https://mega.nz/file/Abcd1234#{}",
            URL_SAFE_NO_PAD.encode(key)
        ))
        .unwrap();
        let endpoint = Url::parse("http://mega-api.test/cs").unwrap();
        let meta = crate::mega::resolve_at(&url, &opts, &endpoint)
            .await
            .unwrap();
        assert_eq!(meta.name, "Orbit MEGA test.bin");
        assert_eq!(meta.size, body.len() as u64);
        let dest = folder.path().join("Downloads").join(&meta.name);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let cache = base(&url, &dest, &opts);
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let reservation = naming::reserve(&dest).unwrap();
        let mut part = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(naming::part_path(&cache))
            .unwrap();
        part.set_len(meta.size).unwrap();
        let path = dest.clone();
        assert!(!state_path(&dest).exists());
        assert!(!naming::part_path(&dest).exists());
        part.write_all(&body[..cursor as usize]).unwrap();
        let saved = ResumeState::new(
            url.clone(),
            url.clone(),
            path,
            meta.size,
            Validators::default(),
        );
        let snapshot = Arc::new(Mutex::new(None));
        let observed = snapshot.clone();
        let dl = Download::spawn(url, move |shared, cancel| async move {
            let result = run(
                receive(
                    meta,
                    saved,
                    part,
                    reservation,
                    opts,
                    shared.clone(),
                    cancel,
                    cursor,
                ),
                shared.clone(),
            )
            .await;
            *observed.lock().unwrap() = Some(shared.snapshot());
            result
        });
        let result = dl.wait().await;
        let requests = seen.lock().unwrap().clone();
        server.abort();
        assert!(
            requests
                .iter()
                .any(|r| r.starts_with("http://mega-api.test/cs?"))
        );
        assert!(
            requests
                .iter()
                .any(|r| r.starts_with(&format!("http://mega-storage.test/object/{cursor}-")))
        );
        if let Ok(out) = &result {
            assert_eq!(std::fs::read(&out.path).unwrap(), body);
            assert!(!state_path(&cache).exists());
            assert!(!naming::part_path(&dest).exists());
            let expected = SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000);
            assert_eq!(
                std::fs::metadata(&dest).unwrap().modified().unwrap(),
                expected
            );
        } else {
            assert!(!dest.exists());
            assert!(state_path(&cache).exists());
            assert!(naming::part_path(&cache).exists());
            assert!(!state_path(&dest).exists());
            assert!(!naming::part_path(&dest).exists());
        }
        result.map(|_| ())
    }

    #[tokio::test]
    async fn mega_public_folders_decrypt_node_keys_and_limit_selected_subfolders() {
        let (proxy, _, server) = proxy(Vec::new(), false, false).await;
        let opts = Options {
            proxy: Proxy::Manual {
                url: proxy,
                login: Some(("test".into(), "pass".into())),
                http: true,
                https: true,
            },
            ..Default::default()
        };
        let endpoint = Url::parse("http://folder-api.test/cs").unwrap();
        let base = format!(
            "https://mega.nz/folder/Folder12#{}",
            URL_SAFE_NO_PAD.encode([7u8; 16])
        );
        let files = crate::mega::folder_at(&Url::parse(&base).unwrap(), &opts, &endpoint)
            .await
            .unwrap();
        assert_eq!(files.len(), 2);
        for file in files {
            assert!(
                matches!(crate::mega::parse(&file).unwrap(),crate::mega::Link::File { key,folder:Some(folder),.. } if hex::encode(key)==KEY && folder=="Folder12")
            );
        }
        let files = crate::mega::folder_at(
            &Url::parse(&(base.clone() + "/folder/SubFold1")).unwrap(),
            &opts,
            &endpoint,
        )
        .await
        .unwrap();
        assert_eq!(files.len(), 1);
        assert!(
            matches!(crate::mega::parse(&files[0]).unwrap(),crate::mega::Link::File {id,..} if id=="File2222")
        );
        let files = crate::mega::folder_at(
            &Url::parse(&(base + "/file/Abcd1234")).unwrap(),
            &opts,
            &endpoint,
        )
        .await
        .unwrap();
        assert_eq!(files.len(), 1);
        assert!(
            matches!(crate::mega::parse(&files[0]).unwrap(),crate::mega::Link::File {id,..} if id=="Abcd1234")
        );
        server.abort();
    }

    #[tokio::test]
    async fn mega_download_and_unaligned_resume_use_authenticated_proxy_and_verify_plaintext() {
        transfer(false, false, 0).await.unwrap();
        transfer(false, false, 500003).await.unwrap();
    }
    #[tokio::test]
    async fn mega_corruption_never_completes_and_quota_keeps_resume_data() {
        assert!(matches!(
            transfer(true, false, 0).await,
            Err(Error::ChecksumMismatch("MEGA CBC-MAC"))
        ));
        assert!(matches!(
            transfer(false, true, 500003).await,
            Err(Error::Mega(_))
        ));
    }
}
