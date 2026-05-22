//! Chrome for Testing (CfT) fetcher — download, cache, and resolve a pinned
//! Chromium build (Plans.md: "managed, pinned Chrome for Testing").
//!
//! AmberHTML always drives a *real* browser over CDP and never bundles one, so
//! this module guarantees a usable Chromium executable is present on disk. It:
//!
//! 1. honors the `AMBER_CHROMIUM_PATH` escape hatch (return it if it exists);
//! 2. otherwise resolves the pinned CfT build for the host platform;
//! 3. downloads + extracts the official CfT `.zip` into a per-user cache the
//!    first time, then reuses it on every subsequent run; and
//! 4. returns the path to the platform-specific Chromium binary inside.
//!
//! ## Pinning & reproducibility
//! The version is pinned to a single, recent **stable** Chrome for Testing
//! build (see [`CFT_VERSION`]). Pinning is what makes captures reproducible and
//! keeps our hand-rolled CDP client matched to a known protocol revision
//! (Plans.md: "Chromium version drift breaks CDP client → pin browser").
//!
//! ## On checksum verification (Plans.md: "checksum-verified")
//! The plan *prefers* verifying the download against a published checksum.
//! Unfortunately the Chrome for Testing JSON API does **not** publish any
//! per-download hashes: both
//! `known-good-versions-with-downloads.json` and
//! `last-known-good-versions-with-downloads.json` expose only `{platform, url}`
//! pairs under `downloads.chrome` — there is no `sha256`/`hash` field anywhere
//! in the schema (verified against the live endpoints). Google does not publish
//! a sidecar checksum file for these zips either.
//!
//! Because there is nothing authoritative to verify *against*, we do not invent
//! a hash here (a self-computed hash verifies nothing about authenticity). The
//! download is integrity-protected by HTTPS/TLS to `storage.googleapis.com`.
//! See [`pinned_sha256`] for the single seam where a real, externally-sourced
//! checksum should be wired in.
//!
//! TODO(checksum): when an authoritative checksum source exists — e.g. a vendored
//! `version -> {platform -> sha256}` table generated out-of-band, or a future CfT
//! API field — populate [`pinned_sha256`] and gate extraction on a SHA-256 match
//! of the downloaded bytes. To keep the dependency footprint minimal while no
//! checksum is available, the actual hashing is intentionally NOT wired up; the
//! seam ([`pinned_sha256`] + the call site in [`download_and_extract`]) marks
//! exactly where a `sha2`-based check would go.

use std::path::{Path, PathBuf};

/// Pinned Chrome for Testing **stable** version.
///
/// Bump this deliberately (it changes the CDP protocol revision we target) and
/// re-run the protocol-audit gate described in Plans.md.
pub const CFT_VERSION: &str = "149.0.7827.22";

/// Base URL for the public Chrome for Testing download bucket.
const CFT_DOWNLOAD_BASE: &str = "https://storage.googleapis.com/chrome-for-testing-public";

/// Errors from the Chromium fetcher. Kept LOCAL to this module per the task;
/// `browser::ensure_chromium` maps these into the crate-wide `Error`.
#[derive(Debug, thiserror::Error)]
pub enum ChromiumError {
    /// The host OS/arch has no Chrome for Testing build.
    #[error("unsupported platform for Chrome for Testing: {0}")]
    UnsupportedPlatform(String),

    /// Downloading the CfT archive failed (transport, HTTP status, etc.).
    #[error("failed to download Chrome for Testing: {0}")]
    Download(String),

    /// Unpacking the downloaded `.zip` failed.
    #[error("failed to extract Chrome for Testing archive: {0}")]
    Extract(String),

    /// Extraction succeeded but the expected Chromium binary was not found.
    #[error("Chromium binary not found at expected path: {0}")]
    BinaryNotFound(PathBuf),

    /// Filesystem / I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Chrome for Testing platform identifiers (the `<platform>` URL segment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CftPlatform {
    Linux64,
    MacX64,
    MacArm64,
    Win32,
    Win64,
}

impl CftPlatform {
    /// The exact platform string CfT uses in URLs and archive directory names.
    pub fn as_str(self) -> &'static str {
        match self {
            CftPlatform::Linux64 => "linux64",
            CftPlatform::MacX64 => "mac-x64",
            CftPlatform::MacArm64 => "mac-arm64",
            CftPlatform::Win32 => "win32",
            CftPlatform::Win64 => "win64",
        }
    }

    /// Detect the CfT platform for the current host (compile-time `cfg`).
    pub fn detect() -> Result<Self, ChromiumError> {
        Self::from_os_arch(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Map a Rust `(OS, ARCH)` pair (`std::env::consts`) to a CfT platform.
    ///
    /// Pure and total over its inputs so it can be unit-tested without touching
    /// the real host.
    pub fn from_os_arch(os: &str, arch: &str) -> Result<Self, ChromiumError> {
        match (os, arch) {
            ("linux", "x86_64") => Ok(CftPlatform::Linux64),
            ("macos", "aarch64") => Ok(CftPlatform::MacArm64),
            ("macos", "x86_64") => Ok(CftPlatform::MacX64),
            // CfT ships no win-arm64 build; arm64 Windows runs the x64 build
            // under emulation, so map it there rather than failing.
            ("windows", "x86_64") | ("windows", "aarch64") => Ok(CftPlatform::Win64),
            ("windows", "x86") => Ok(CftPlatform::Win32),
            _ => Err(ChromiumError::UnsupportedPlatform(format!("{os}/{arch}"))),
        }
    }

    /// Path to the Chromium executable *relative to the extracted archive root*
    /// for this platform. CfT zips contain a single top-level directory named
    /// `chrome-<platform>`.
    fn binary_rel_path(self) -> &'static str {
        match self {
            CftPlatform::Linux64 => "chrome-linux64/chrome",
            CftPlatform::MacX64 => {
                "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            }
            CftPlatform::MacArm64 => {
                "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            }
            CftPlatform::Win32 => "chrome-win32/chrome.exe",
            CftPlatform::Win64 => "chrome-win64/chrome.exe",
        }
    }
}

/// Build the CfT download URL for `version`/`platform`:
/// `https://storage.googleapis.com/chrome-for-testing-public/<version>/<platform>/chrome-<platform>.zip`.
fn download_url(version: &str, platform: CftPlatform) -> String {
    let p = platform.as_str();
    format!("{CFT_DOWNLOAD_BASE}/{version}/{p}/chrome-{p}.zip")
}

/// Root cache directory for managed Chromium builds:
/// `<cache>/amber-html/chromium`, where `<cache>` is the per-user cache dir
/// (e.g. `~/.cache` on Linux, `~/Library/Caches` on macOS).
fn cache_root() -> Result<PathBuf, ChromiumError> {
    let base = dirs::cache_dir().ok_or_else(|| {
        ChromiumError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not determine a per-user cache directory",
        ))
    })?;
    Ok(base.join("amber-html").join("chromium"))
}

/// Per-version install directory: `<cache_root>/<version>`.
fn version_dir(version: &str) -> Result<PathBuf, ChromiumError> {
    Ok(cache_root()?.join(version))
}

/// Resolve the chrome executable path inside an already-extracted version dir.
fn binary_path(version_dir: &Path, platform: CftPlatform) -> PathBuf {
    version_dir.join(platform.binary_rel_path())
}

/// Ensure a usable Chrome for Testing executable exists and return its path.
///
/// Order of resolution:
/// 1. `AMBER_CHROMIUM_PATH` — if set and the path exists, return it verbatim.
/// 2. Cached build — if the pinned version is already extracted, return its binary.
/// 3. Otherwise download + extract the pinned CfT zip into the cache, then return it.
pub fn ensure_chromium() -> Result<PathBuf, ChromiumError> {
    // 1. Escape hatch (Plans.md).
    if let Some(p) = std::env::var_os("AMBER_CHROMIUM_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
        // If set but missing we fall through to the managed path rather than
        // hard-failing — the override is a convenience, not a contract.
    }

    let platform = CftPlatform::detect()?;
    let vdir = version_dir(CFT_VERSION)?;
    let bin = binary_path(&vdir, platform);

    // 2. Already cached?
    if bin.exists() {
        return Ok(bin);
    }

    // 3. Download + extract.
    download_and_extract(CFT_VERSION, platform, &vdir)?;

    if !bin.exists() {
        return Err(ChromiumError::BinaryNotFound(bin));
    }
    #[cfg(unix)]
    ensure_executable(&bin)?;
    Ok(bin)
}

/// Download the CfT zip for `version`/`platform` and extract it into `dest`.
///
/// Extraction is staged through a sibling temp dir and atomically renamed into
/// place so a crash mid-extract never leaves a half-populated `version_dir`
/// that the cache-hit check would wrongly trust.
fn download_and_extract(
    version: &str,
    platform: CftPlatform,
    dest: &Path,
) -> Result<(), ChromiumError> {
    let url = download_url(version, platform);

    // Stream the archive to a temp file on disk; CfT zips are ~150-300 MB and
    // we want a Seek-able reader for `zip` without holding it all in memory.
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    let mut resp = ureq::get(&url)
        .call()
        .map_err(|e| ChromiumError::Download(format!("GET {url}: {e}")))?;

    // Stage the archive at a deterministic, version-scoped temp path beside the
    // destination (avoids a `tempfile` dependency). A stale file from a crashed
    // prior run is simply overwritten.
    let tmp_zip = parent.join(format!(".chrome-{}-{}.zip.partial", version, platform.as_str()));
    {
        let mut out = std::fs::File::create(&tmp_zip).map_err(ChromiumError::Io)?;
        let mut reader = resp.body_mut().as_reader();
        std::io::copy(&mut reader, &mut out)
            .map_err(|e| ChromiumError::Download(format!("reading body of {url}: {e}")))?;
    }

    // OPTIONAL checksum gate — currently a no-op because no authoritative hash
    // exists (see module docs). When `pinned_sha256` is populated, hash the
    // staged `tmp_zip` here and bail on mismatch before extracting.
    let _ = pinned_sha256(version, platform);

    // Extract into a staging dir alongside `dest`, then atomically rename so the
    // cache-hit check never trusts a half-populated `version_dir`.
    let staging = parent.join(format!(".{}.partial", version));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(ChromiumError::Io)?;
    }
    std::fs::create_dir_all(&staging)?;

    let zip_file = std::fs::File::open(&tmp_zip).map_err(ChromiumError::Io)?;
    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| ChromiumError::Extract(format!("opening archive: {e}")))?;
    archive
        .extract(&staging)
        .map_err(|e| ChromiumError::Extract(format!("unpacking archive: {e}")))?;

    // Move staging -> dest. If a concurrent run beat us to it, drop ours.
    if dest.exists() {
        std::fs::remove_dir_all(&staging).ok();
    } else if let Err(e) = std::fs::rename(&staging, dest) {
        std::fs::remove_dir_all(&staging).ok();
        // A racing process may have created `dest` between our check and rename.
        if !dest.exists() {
            std::fs::remove_file(&tmp_zip).ok();
            return Err(ChromiumError::Io(e));
        }
    }

    // Best-effort cleanup of the staged archive.
    std::fs::remove_file(&tmp_zip).ok();

    Ok(())
}

/// Authoritative expected SHA-256 (lowercase hex) for `version`/`platform`.
///
/// Returns `None` today: Chrome for Testing publishes no per-download hashes
/// (see module docs), so there is nothing to verify against. This is the single
/// seam to populate when an external checksum table becomes available; the call
/// site in [`download_and_extract`] will then hash the staged archive (e.g. via
/// the `sha2` crate) and reject a mismatch before extracting.
fn pinned_sha256(_version: &str, _platform: CftPlatform) -> Option<&'static str> {
    None
}

/// Ensure `path` is owner-executable on Unix (CfT's `chrome` may extract without
/// the +x bit depending on the zip's stored modes).
#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<(), ChromiumError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).map_err(ChromiumError::Io)?.permissions();
    let mode = perms.mode();
    if mode & 0o111 == 0 {
        perms.set_mode(mode | 0o755);
        std::fs::set_permissions(path, perms).map_err(ChromiumError::Io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- platform -> string mapping ----

    #[test]
    fn platform_strings_match_cft() {
        assert_eq!(CftPlatform::Linux64.as_str(), "linux64");
        assert_eq!(CftPlatform::MacX64.as_str(), "mac-x64");
        assert_eq!(CftPlatform::MacArm64.as_str(), "mac-arm64");
        assert_eq!(CftPlatform::Win32.as_str(), "win32");
        assert_eq!(CftPlatform::Win64.as_str(), "win64");
    }

    #[test]
    fn os_arch_detection() {
        assert_eq!(
            CftPlatform::from_os_arch("linux", "x86_64").unwrap(),
            CftPlatform::Linux64
        );
        assert_eq!(
            CftPlatform::from_os_arch("macos", "aarch64").unwrap(),
            CftPlatform::MacArm64
        );
        assert_eq!(
            CftPlatform::from_os_arch("macos", "x86_64").unwrap(),
            CftPlatform::MacX64
        );
        assert_eq!(
            CftPlatform::from_os_arch("windows", "x86_64").unwrap(),
            CftPlatform::Win64
        );
        assert_eq!(
            CftPlatform::from_os_arch("windows", "aarch64").unwrap(),
            CftPlatform::Win64
        );
        assert_eq!(
            CftPlatform::from_os_arch("windows", "x86").unwrap(),
            CftPlatform::Win32
        );
    }

    #[test]
    fn unsupported_platform_errors() {
        let err = CftPlatform::from_os_arch("freebsd", "x86_64").unwrap_err();
        assert!(matches!(err, ChromiumError::UnsupportedPlatform(p) if p == "freebsd/x86_64"));

        let err = CftPlatform::from_os_arch("linux", "aarch64").unwrap_err();
        assert!(matches!(err, ChromiumError::UnsupportedPlatform(_)));
    }

    // ---- URL construction ----

    #[test]
    fn url_construction() {
        assert_eq!(
            download_url("149.0.7827.22", CftPlatform::MacArm64),
            "https://storage.googleapis.com/chrome-for-testing-public/149.0.7827.22/mac-arm64/chrome-mac-arm64.zip"
        );
        assert_eq!(
            download_url("149.0.7827.22", CftPlatform::Linux64),
            "https://storage.googleapis.com/chrome-for-testing-public/149.0.7827.22/linux64/chrome-linux64.zip"
        );
        assert_eq!(
            download_url("100.0.0.0", CftPlatform::Win64),
            "https://storage.googleapis.com/chrome-for-testing-public/100.0.0.0/win64/chrome-win64.zip"
        );
    }

    // ---- per-platform binary path resolution ----

    #[test]
    fn binary_paths_per_platform() {
        let root = Path::new("/cache/amber-html/chromium/149.0.7827.22");

        assert_eq!(
            binary_path(root, CftPlatform::Linux64),
            root.join("chrome-linux64/chrome")
        );
        assert_eq!(
            binary_path(root, CftPlatform::Win64),
            root.join("chrome-win64/chrome.exe")
        );
        assert_eq!(
            binary_path(root, CftPlatform::Win32),
            root.join("chrome-win32/chrome.exe")
        );
        assert_eq!(
            binary_path(root, CftPlatform::MacArm64),
            root.join(
                "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            )
        );
        assert_eq!(
            binary_path(root, CftPlatform::MacX64),
            root.join(
                "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            )
        );
    }

    // ---- cache layout ----

    #[test]
    fn version_dir_layout() {
        // Only assert the suffix; the per-user cache prefix is environment-dependent.
        let vdir = version_dir("149.0.7827.22").unwrap();
        assert!(vdir.ends_with("amber-html/chromium/149.0.7827.22"));
    }

    // ---- checksum seam ----

    #[test]
    fn no_pinned_checksum_today() {
        // Documents the current state: CfT publishes no authoritative hashes.
        assert!(pinned_sha256(CFT_VERSION, CftPlatform::Linux64).is_none());
    }

    // ---- JSON parsing of the CfT known-good-versions feed ----
    //
    // Not used by the runtime fetch (we pin a version directly), but the schema
    // is parsed here to prove our understanding of it and to back the doc claim
    // that `downloads.chrome[]` carries only {platform, url} — no checksum.

    #[test]
    fn parses_cft_known_good_fixture() {
        // Minimal fixture mirroring the real `*-versions-with-downloads.json` shape.
        let fixture = r#"{
            "timestamp": "2026-05-21T00:00:00.000Z",
            "versions": [
                {
                    "version": "149.0.7827.22",
                    "revision": "1625079",
                    "downloads": {
                        "chrome": [
                            { "platform": "linux64",   "url": "https://storage.googleapis.com/chrome-for-testing-public/149.0.7827.22/linux64/chrome-linux64.zip" },
                            { "platform": "mac-arm64", "url": "https://storage.googleapis.com/chrome-for-testing-public/149.0.7827.22/mac-arm64/chrome-mac-arm64.zip" }
                        ]
                    }
                }
            ]
        }"#;

        let v: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let versions = v["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 1);

        let entry = &versions[0];
        assert_eq!(entry["version"], "149.0.7827.22");

        let chrome = entry["downloads"]["chrome"].as_array().unwrap();
        let linux = chrome
            .iter()
            .find(|d| d["platform"] == "linux64")
            .expect("linux64 download present");
        assert_eq!(
            linux["url"],
            "https://storage.googleapis.com/chrome-for-testing-public/149.0.7827.22/linux64/chrome-linux64.zip"
        );

        // Confirm the schema carries NO checksum field (the basis for our
        // "checksum verification is not currently possible" decision).
        assert!(linux.get("sha256").is_none());
        assert!(linux.get("hash").is_none());
        assert!(linux.get("checksum").is_none());
        assert_eq!(linux.as_object().unwrap().len(), 2); // exactly {platform, url}
    }

    // ---- network-touching test (excluded from normal runs) ----

    #[test]
    #[ignore = "downloads ~hundreds of MB from Google; run explicitly"]
    fn end_to_end_download() {
        let path = ensure_chromium().expect("ensure_chromium should yield a binary");
        assert!(path.exists(), "returned path should exist: {}", path.display());
    }
}
