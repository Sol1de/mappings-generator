//! HTTP client and verified cache (spec/pipeline.md#cache-and-integrity) shared by providers.
//!
//! Every download lands in a content-addressed store (`objects/<sha256>`). Mutable metadata
//! is re-fetched after a TTL; `--offline` only reads the cache; `--locked` only accepts the
//! URL → SHA-256 pairs recorded in a lock file.

pub mod fabric;
pub mod forge;
pub mod lunar;
pub mod mojang;

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const USER_AGENT: &str = concat!("mc_mappings_generator/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    Network(String),
    NotFound(String),
    Integrity(String),
    Offline(String),
    Locked(String),
    Io(String),
    Format(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(m) => write!(f, "network error: {m}"),
            Self::NotFound(m) => write!(f, "not found: {m}"),
            Self::Integrity(m) => write!(f, "integrity error: {m}"),
            Self::Offline(m) => write!(f, "offline: not in cache: {m}"),
            Self::Locked(m) => write!(f, "locked: {m}"),
            Self::Io(m) => write!(f, "cache I/O error: {m}"),
            Self::Format(m) => write!(f, "unexpected upstream data: {m}"),
        }
    }
}

impl std::error::Error for SourceError {}

pub type Result<T> = std::result::Result<T, SourceError>;

fn io_err(path: &Path, e: std::io::Error) -> SourceError {
    SourceError::Io(format!("{}: {e}", path.display()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn sha1(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

/// One input of a generation, recorded in provenance and in the lock file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub license: String,
    #[serde(skip)]
    pub path: PathBuf,
}

impl Artifact {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.path).map_err(|e| io_err(&self.path, e))
    }

    pub fn text(&self) -> Result<String> {
        String::from_utf8(self.bytes()?)
            .map_err(|_| SourceError::Format(format!("{} is not UTF-8", self.url)))
    }

    pub fn json(&self) -> Result<serde_json::Value> {
        serde_json::from_slice(&self.bytes()?)
            .map_err(|e| SourceError::Format(format!("{}: {e}", self.url)))
    }

    /// Reads one entry of a zip artifact.
    pub fn zip_entry(&self, name: &str) -> Result<Vec<u8>> {
        let file = std::fs::File::open(&self.path).map_err(|e| io_err(&self.path, e))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| SourceError::Format(format!("{}: {e}", self.url)))?;
        let mut entry = zip
            .by_name(name)
            .map_err(|_| SourceError::Format(format!("{} has no {name}", self.url)))?;
        let mut out = Vec::new();
        entry
            .read_to_end(&mut out)
            .map_err(|e| SourceError::Format(format!("{}!{name}: {e}", self.url)))?;
        Ok(out)
    }
}

/// Hash published by upstream for an immutable artifact.
#[derive(Clone, Debug)]
pub enum Expected {
    Sha1(String),
    /// Mutable metadata, re-fetched after the TTL.
    Mutable,
    /// Immutable artifact without a published hash.
    Unpublished,
}

pub struct Cache {
    pub dir: PathBuf,
    pub offline: bool,
    pub ttl: Duration,
    /// URL → SHA-256 accepted under `--locked`.
    pub lock: Option<BTreeMap<String, String>>,
    /// Every URL fetched during this run → SHA-256, for the lock file.
    pub log: std::sync::Mutex<BTreeMap<String, String>>,
    agent: ureq::Agent,
}

static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn key(url: &str) -> String {
    sha256(url.as_bytes())
}

impl Cache {
    pub fn new(dir: Option<PathBuf>, offline: bool) -> Result<Self> {
        let dir = match dir {
            Some(d) => d,
            None => std::env::var_os("LOCALAPPDATA")
                .map(|d| PathBuf::from(d).join("mc_mappings_generator"))
                .ok_or_else(|| {
                    SourceError::Io("LOCALAPPDATA is not set; use --cache-dir".into())
                })?,
        };
        for sub in ["objects", "urls", "tmp"] {
            std::fs::create_dir_all(dir.join(sub)).map_err(|e| io_err(&dir, e))?;
        }
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(120))
            .build();
        Ok(Self {
            dir,
            offline,
            ttl: Duration::from_secs(3600),
            lock: None,
            log: Default::default(),
            agent,
        })
    }

    pub fn object_path(&self, sha: &str) -> PathBuf {
        self.dir.join("objects").join(sha)
    }

    /// Downloads with retries and exponential backoff on transient errors.
    fn download(&self, url: &str) -> Result<Vec<u8>> {
        let mut delay = Duration::from_millis(500);
        for attempt in 1..=4 {
            match self.agent.get(url).call() {
                Ok(resp) => {
                    let mut body = Vec::new();
                    match resp.into_reader().take(2 << 30).read_to_end(&mut body) {
                        Ok(_) => return Ok(body),
                        Err(e) if attempt == 4 => {
                            return Err(SourceError::Network(format!("{url}: {e}")));
                        }
                        Err(_) => {}
                    }
                }
                Err(ureq::Error::Status(404 | 410, _)) => {
                    return Err(SourceError::NotFound(url.to_owned()));
                }
                Err(ureq::Error::Status(code, _)) if code != 429 && code < 500 => {
                    return Err(SourceError::Network(format!("{url}: HTTP {code}")));
                }
                Err(e) if attempt == 4 => return Err(SourceError::Network(format!("{url}: {e}"))),
                Err(_) => {}
            }
            std::thread::sleep(delay);
            delay *= 2;
        }
        unreachable!("last attempt returns")
    }

    pub fn store_bytes(&self, bytes: &[u8]) -> Result<(String, PathBuf)> {
        let sha = sha256(bytes);
        let path = self.object_path(&sha);
        if !path.exists() {
            let n = TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp = self
                .dir
                .join("tmp")
                .join(format!("{sha}.{}.{n}", std::process::id()));
            std::fs::write(&tmp, bytes).map_err(|e| io_err(&tmp, e))?;
            if std::fs::rename(&tmp, &path).is_err() && !path.exists() {
                return Err(SourceError::Io(format!("cannot store {}", path.display())));
            }
            let _ = std::fs::remove_file(&tmp);
        }
        Ok((sha, path))
    }

    fn verified(&self, sha: &str, url: &str) -> Result<Option<(PathBuf, u64)>> {
        let path = self.object_path(sha);
        match std::fs::read(&path) {
            Ok(bytes) if sha256(&bytes) == sha => Ok(Some((path, bytes.len() as u64))),
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
                Err(SourceError::Integrity(format!(
                    "cached copy of {url} is corrupted; removed, retry"
                )))
            }
            Err(_) => Ok(None),
        }
    }

    /// Fetches `url` through the cache and checks it against `expected`.
    pub fn fetch(
        &self,
        id: &str,
        url: &str,
        expected: Expected,
        license: &str,
    ) -> Result<Artifact> {
        let a = self.fetch_inner(id, url, expected, license)?;
        self.log
            .lock()
            .unwrap()
            .insert(url.to_owned(), a.sha256.clone());
        Ok(a)
    }

    /// The cached copy of `url`, without any download.
    pub fn cached(&self, url: &str) -> Option<Artifact> {
        let sha = std::fs::read_to_string(self.dir.join("urls").join(key(url))).ok()?;
        let (path, size) = self.verified(sha.trim(), url).ok()??;
        Some(Artifact {
            id: String::new(),
            url: url.to_owned(),
            size,
            sha256: sha.trim().to_owned(),
            license: String::new(),
            path,
        })
    }

    fn fetch_inner(
        &self,
        id: &str,
        url: &str,
        expected: Expected,
        license: &str,
    ) -> Result<Artifact> {
        let pointer = self.dir.join("urls").join(key(url));
        let make = |sha: String, path: PathBuf, size: u64| Artifact {
            id: id.to_owned(),
            url: url.to_owned(),
            size,
            sha256: sha,
            license: license.to_owned(),
            path,
        };
        if let Some(lock) = &self.lock {
            let Some(sha) = lock.get(url) else {
                return Err(SourceError::Locked(format!(
                    "{url} is not in the lock file"
                )));
            };
            if let Some((path, size)) = self.verified(sha, url)? {
                return Ok(make(sha.clone(), path, size));
            }
            if self.offline {
                return Err(SourceError::Offline(url.to_owned()));
            }
            let bytes = self.download(url)?;
            let (got, path) = self.store_bytes(&bytes)?;
            if &got != sha {
                return Err(SourceError::Locked(format!(
                    "{url} changed upstream: {got} != locked {sha}"
                )));
            }
            return Ok(make(got, path, bytes.len() as u64));
        }
        if let Ok(text) = std::fs::read_to_string(&pointer) {
            let sha = text.trim().to_owned();
            let fresh = !matches!(expected, Expected::Mutable)
                || self.offline
                || std::fs::metadata(&pointer)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| {
                        SystemTime::now().duration_since(t).unwrap_or_default() < self.ttl
                    });
            if fresh && let Some((path, size)) = self.verified(&sha, url)? {
                return Ok(make(sha, path, size));
            }
        }
        if self.offline {
            return Err(SourceError::Offline(url.to_owned()));
        }
        let bytes = self.download(url)?;
        let ok = match &expected {
            Expected::Sha1(h) => sha1(&bytes).eq_ignore_ascii_case(h),
            Expected::Mutable | Expected::Unpublished => true,
        };
        if !ok {
            return Err(SourceError::Integrity(format!(
                "{url} does not match its published hash {expected:?}"
            )));
        }
        let (sha, path) = self.store_bytes(&bytes)?;
        std::fs::write(&pointer, &sha).map_err(|e| io_err(&pointer, e))?;
        Ok(make(sha, path, bytes.len() as u64))
    }

    /// Fetches a Maven artifact, verified against its published `.sha1` when there is one.
    pub fn maven(&self, id: &str, url: &str, license: &str) -> Result<Artifact> {
        if self.lock.is_some() {
            return self.fetch(id, url, Expected::Unpublished, license);
        }
        let expected = match self.fetch(
            &format!("{id}.sha1"),
            &format!("{url}.sha1"),
            Expected::Unpublished,
            "",
        ) {
            Ok(a) => Expected::Sha1(a.text()?.split_whitespace().next().unwrap_or("").to_owned()),
            Err(SourceError::NotFound(_)) => Expected::Unpublished,
            Err(e) => return Err(e),
        };
        self.fetch(id, url, expected, license)
    }

    /// Size and number of cached objects.
    pub fn info(&self) -> Result<(u64, u64)> {
        let mut total = (0, 0);
        let dir = self.dir.join("objects");
        for e in std::fs::read_dir(&dir)
            .map_err(|e| io_err(&dir, e))?
            .flatten()
        {
            total.0 += 1;
            total.1 += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
        Ok(total)
    }

    /// Removes cached objects and pointers older than `older_than` (everything if `None`).
    pub fn clean(&self, older_than: Option<Duration>) -> Result<u64> {
        let mut removed = 0;
        for sub in ["objects", "urls", "tmp"] {
            let dir = self.dir.join(sub);
            for e in std::fs::read_dir(&dir)
                .map_err(|e| io_err(&dir, e))?
                .flatten()
            {
                let old = older_than.is_none_or(|d| {
                    e.metadata()
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() > d)
                });
                if old && std::fs::remove_file(e.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }
}

/// Numeric-aware version ordering (`1.21.10` > `1.21.9`, `47.4.29` > `47.4.3`).
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| -> Vec<(u64, String)> {
        s.split(['.', '-', '+'])
            .map(|p| {
                let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
                (digits.parse().unwrap_or(0), p[digits.len()..].to_owned())
            })
            .collect()
    };
    parts(a).cmp(&parts(b))
}

/// `<version>` values of a Maven `maven-metadata.xml`.
pub fn maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split_once("</version>").map(|(v, _)| v.trim().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(offline: bool) -> Cache {
        let dir =
            std::env::temp_dir().join(format!("mmg-cache-test-{}-{offline}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Cache::new(Some(dir), offline).unwrap()
    }

    #[test]
    fn offline_miss_is_explicit_and_hits_are_verified() {
        let cache = temp_cache(true);
        let err = cache
            .fetch("x", "https://example.invalid/x.json", Expected::Mutable, "")
            .unwrap_err();
        assert!(matches!(err, SourceError::Offline(_)));
        let (sha, path) = cache.store_bytes(b"hello").unwrap();
        std::fs::write(
            cache
                .dir
                .join("urls")
                .join(key("https://example.invalid/x.json")),
            &sha,
        )
        .unwrap();
        let a = cache
            .fetch("x", "https://example.invalid/x.json", Expected::Mutable, "")
            .unwrap();
        assert_eq!((a.size, a.sha256.as_str()), (5, sha.as_str()));
        std::fs::write(&path, b"tampered").unwrap();
        assert!(matches!(
            cache.fetch("x", "https://example.invalid/x.json", Expected::Mutable, ""),
            Err(SourceError::Integrity(_))
        ));
    }

    #[test]
    fn locked_rejects_unknown_urls() {
        let mut cache = temp_cache(false);
        cache.lock = Some(BTreeMap::new());
        assert!(matches!(
            cache.fetch("x", "https://example.invalid/y", Expected::Unpublished, ""),
            Err(SourceError::Locked(_))
        ));
    }

    #[test]
    fn helpers() {
        assert_eq!(sha1(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            version_cmp("1.21.10", "1.21.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            version_cmp("47.4.29", "47.4.3"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            version_cmp("21.0.166", "21.0.104-beta"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            maven_versions("<versions><version>1</version>\n<version> 2 </version></versions>"),
            ["1", "2"]
        );
    }
}
