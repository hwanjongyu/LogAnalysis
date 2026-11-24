use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::*;
use serde::{Deserialize, Serialize};

use super::filters::write_file_atomically;
use super::pattern_history::read_to_string_bounded;
use super::recent_files::config_file_path;
use super::LogAnalysisWorkspace;
use crate::types::InputFocusTarget;

/// Current on-disk schema version for `update_settings.json`.
pub const UPDATE_SETTINGS_VERSION: u32 = 1;
/// Throttle interval between automatic startup update checks (24 hours).
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 86_400;
/// Maximum allowed file size when reading `update_settings.json` (64 KiB).
pub const MAX_UPDATE_SETTINGS_BYTES: usize = 64 * 1024;
/// Maximum allowed HTTP response body size from GitHub Releases API (256 KiB).
pub const MAX_GITHUB_RESPONSE_BYTES: usize = 256 * 1024;
/// Maximum number of Unicode scalar characters retained in release notes preview.
pub const MAX_RELEASE_NOTES_CHARS: usize = 4_000;
/// Timeout for silent background startup update checks.
pub const STARTUP_CHECK_TIMEOUT_SECS: u64 = 5;
/// Timeout for user-initiated manual update checks.
pub const MANUAL_CHECK_TIMEOUT_SECS: u64 = 8;
/// Maximum byte length allowed for a validated release URL.
pub const MAX_RELEASE_URL_BYTES: usize = 512;
/// Maximum bytes drained from the non-API fallback `GET` response body.
const MAX_FALLBACK_BODY_DRAIN_BYTES: usize = 16 * 1024;

/// Primary GitHub REST API endpoint for the latest published release.
pub const GITHUB_API_LATEST_URL: &str =
    "https://api.github.com/repos/hwanjongyu/LogAnalysis/releases/latest";
/// Secondary web endpoint used for single-hop redirect fallback on rate limits (`403`/`429`).
pub const GITHUB_WEB_LATEST_URL: &str =
    "https://github.com/hwanjongyu/LogAnalysis/releases/latest";
/// Strict URL prefix allowlist for release pages opened in the OS browser.
pub const ALLOWED_RELEASE_URL_PREFIX: &str =
    "https://github.com/hwanjongyu/LogAnalysis/releases";

static UPDATE_SETTINGS_SAVE_LOCK: Mutex<()> = Mutex::new(());
static UPDATE_SETTINGS_SAVE_GEN: AtomicU64 = AtomicU64::new(0);

fn default_schema_version() -> u32 {
    UPDATE_SETTINGS_VERSION
}

fn default_true() -> bool {
    true
}

/// Core semantic version `(major, minor, patch)` parsed strictly for stable releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVerCore(pub u64, pub u64, pub u64);

/// Release downloadable asset metadata (Stage 2 seam; unused by Stage 1 UI).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// Sanitized GitHub release metadata cached on disk and displayed in the Update Modal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
    /// Stage 2 seam: release assets parsed from GitHub `assets[]` (unused by Stage 1 UI).
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

/// Persisted user preferences and ETag cache stored in `update_settings.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    #[serde(default = "default_schema_version")]
    pub version: u32,
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    pub last_checked_epoch_secs: Option<u64>,
    pub etag: Option<String>,
    pub cached_latest_release: Option<ReleaseInfo>,
    pub skipped_version: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            version: UPDATE_SETTINGS_VERSION,
            auto_check_updates: true,
            last_checked_epoch_secs: None,
            etag: None,
            cached_latest_release: None,
            skipped_version: None,
        }
    }
}

impl UpdateSettings {
    /// Returns `true` if `tag` matches `skipped_version`, normalizing an optional leading `v`/`V`.
    pub fn is_version_skipped(&self, tag: &str) -> bool {
        let Some(skipped) = self.skipped_version.as_deref() else {
            return false;
        };
        if let (Some(a), Some(b)) = (parse_semver_core(skipped), parse_semver_core(tag)) {
            return a == b;
        }
        let norm_skipped = normalize_raw_tag(skipped);
        let norm_tag = normalize_raw_tag(tag);
        !norm_skipped.is_empty() && norm_skipped == norm_tag
    }
}

/// UI state machine for the Stage 1 GitHub Release Update Checker.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum UpdateCheckState {
    #[default]
    Idle,
    Checking,
    UpToDate {
        checked_version: String,
        release: Option<ReleaseInfo>,
    },
    UpdateAvailable(ReleaseInfo),
    Error(String),
}

/// Sanitized error domain for update validation, network checks, and browser launches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    ResponseTooLarge,
    InvalidUrl(String),
    InvalidVersion(String),
    DraftOrPrerelease,
    InvalidJson(String),
    HttpStatus(u16),
    Network(String),
    MissingRedirectLocation,
    TlsUnavailable,
    BrowserLaunch(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::ResponseTooLarge => {
                write!(f, "GitHub release response exceeded size limit.")
            }
            UpdateError::InvalidUrl(reason) => {
                write!(f, "Release URL failed security allowlist: {reason}")
            }
            UpdateError::InvalidVersion(tag) => {
                write!(f, "Release version tag is not a stable SemVer release: {tag}")
            }
            UpdateError::DraftOrPrerelease => {
                write!(f, "Latest GitHub release is a draft or prerelease.")
            }
            UpdateError::InvalidJson(reason) => {
                write!(f, "Invalid GitHub release payload: {reason}")
            }
            UpdateError::HttpStatus(code) => {
                write!(f, "GitHub returned unexpected HTTP status {code}.")
            }
            UpdateError::Network(reason) => {
                write!(f, "Unable to reach GitHub Releases: {reason}")
            }
            UpdateError::MissingRedirectLocation => {
                write!(f, "GitHub fallback response did not include a redirect location.")
            }
            UpdateError::TlsUnavailable => {
                write!(f, "No system TLS root certificates are available.")
            }
            UpdateError::BrowserLaunch(reason) => {
                write!(f, "Could not launch web browser: {reason}")
            }
        }
    }
}

impl std::error::Error for UpdateError {}

#[derive(Debug, Deserialize)]
struct GithubLatestReleasePayload {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

fn normalize_raw_tag(tag: &str) -> &str {
    let trimmed = tag.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
}

/// Formats a tag cleanly with a single lowercase `v` prefix for display.
pub fn format_display_tag(tag: &str) -> String {
    if let Some(SemVerCore(maj, min, pat)) = parse_semver_core(tag) {
        format!("v{maj}.{min}.{pat}")
    } else {
        format!("v{}", normalize_raw_tag(tag))
    }
}

/// Formats an ISO-8601 `published_at` timestamp as `YYYY-MM-DD` when present.
pub fn format_published_date(published_at: Option<&str>) -> Option<String> {
    let raw = published_at?.trim();
    let date_prefix = raw.get(..10)?;
    let bytes = date_prefix.as_bytes();
    if bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        Some(date_prefix.to_string())
    } else {
        None
    }
}

/// Parses a strict stable `SemVerCore` (`[v|V]MAJOR.MINOR.PATCH[+build]`).
/// Rejects pre-release suffixes (`-rc.1`), whitespace, non-ASCII digits, and non-3-part tags.
pub fn parse_semver_core(tag: &str) -> Option<SemVerCore> {
    if tag.is_empty() || tag.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }

    let without_prefix = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);

    if without_prefix.is_empty() || without_prefix.contains('-') {
        return None;
    }

    let core = match without_prefix.split_once('+') {
        Some((c, build)) => {
            if build.is_empty() || !build.is_ascii() {
                return None;
            }
            c
        }
        None => without_prefix,
    };

    let mut parts = core.split('.');
    let major_s = parts.next()?;
    let minor_s = parts.next()?;
    let patch_s = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let parse_component = |s: &str| -> Option<u64> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u64>().ok()
    };

    Some(SemVerCore(
        parse_component(major_s)?,
        parse_component(minor_s)?,
        parse_component(patch_s)?,
    ))
}

/// Returns `true` when `candidate_tag` and `current_version` are valid stable SemVer tags
/// and `candidate_tag` is strictly greater than `current_version`.
pub fn is_newer_version(candidate_tag: &str, current_version: &str) -> bool {
    match (
        parse_semver_core(candidate_tag),
        parse_semver_core(current_version),
    ) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Strictly validates that `raw` is an `https://github.com/hwanjongyu/LogAnalysis/releases...`
/// URL with no whitespace, control chars, backslashes, userinfo, port, or path traversal.
pub fn validate_release_url(raw: &str) -> Result<String, UpdateError> {
    if raw.is_empty() || raw.len() > MAX_RELEASE_URL_BYTES {
        return Err(UpdateError::InvalidUrl(
            "URL is empty or exceeds maximum length".to_string(),
        ));
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UpdateError::InvalidUrl(
            "URL contains whitespace or control characters".to_string(),
        ));
    }
    if raw.contains('\\') {
        return Err(UpdateError::InvalidUrl(
            "URL contains backslash".to_string(),
        ));
    }

    let Some(after_scheme) = raw.strip_prefix("https://") else {
        return Err(UpdateError::InvalidUrl(
            "URL must use https:// scheme".to_string(),
        ));
    };

    let split_idx = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..split_idx];
    let rest = &after_scheme[split_idx..];

    if authority != "github.com" {
        return Err(UpdateError::InvalidUrl(
            "URL authority must be github.com".to_string(),
        ));
    }

    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let path = &rest[..path_end];

    let allowed_repo_path = "/hwanjongyu/LogAnalysis/releases";
    let valid_prefix = path == allowed_repo_path
        || path
            .strip_prefix(allowed_repo_path)
            .is_some_and(|tail| tail.starts_with('/'));
    if !valid_prefix {
        return Err(UpdateError::InvalidUrl(
            "URL path is outside the LogAnalysis releases namespace".to_string(),
        ));
    }

    for segment in path.split('/') {
        if segment == "." || segment == ".." {
            return Err(UpdateError::InvalidUrl(
                "URL path contains relative traversal segments".to_string(),
            ));
        }
    }

    Ok(raw.to_string())
}

/// Sanitizes GitHub release notes into bounded plain text safe for GPUI rendering.
pub fn sanitize_release_notes(raw: &str) -> Option<String> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut final_buf = String::with_capacity(normalized.len().min(MAX_RELEASE_NOTES_CHARS * 4));
    let mut nl_run = 0usize;
    let flush_newlines = |count: usize, out: &mut String| {
        if count == 0 {
            return;
        }
        let emit = if count > 3 { 2 } else { count };
        for _ in 0..emit {
            out.push('\n');
        }
    };

    for ch in normalized.chars() {
        if ch == '\n' {
            nl_run += 1;
            continue;
        }
        if matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
            continue;
        }
        if ch.is_control() && ch != '\t' {
            continue;
        }
        flush_newlines(nl_run, &mut final_buf);
        nl_run = 0;
        if ch == '\t' {
            final_buf.push_str("  ");
        } else {
            final_buf.push(ch);
        }
    }
    flush_newlines(nl_run, &mut final_buf);

    let trimmed = final_buf.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut char_indices = trimmed.char_indices();
    if let Some((byte_idx, _)) = char_indices.nth(MAX_RELEASE_NOTES_CHARS) {
        let truncated = trimmed[..byte_idx].trim_end();
        Some(format!("{truncated}\n… (view full release notes on GitHub)"))
    } else {
        Some(trimmed.to_string())
    }
}

/// Returns `true` when automatic startup checking is enabled and 24h have elapsed.
pub fn should_run_auto_check(settings: &UpdateSettings, now_epoch_secs: u64) -> bool {
    if !settings.auto_check_updates {
        return false;
    }
    match settings.last_checked_epoch_secs {
        None => true,
        Some(last) => now_epoch_secs.saturating_sub(last) >= UPDATE_CHECK_INTERVAL_SECS,
    }
}

/// Emits `If-None-Match` only when both a non-empty ETag and valid cached release exist.
pub fn build_conditional_etag(settings: &UpdateSettings) -> Option<&str> {
    let etag = settings.etag.as_deref().filter(|s| !s.trim().is_empty())?;
    let cached = settings.cached_latest_release.as_ref()?;
    if validate_release_url(&cached.html_url).is_err()
        || parse_semver_core(&cached.tag_name).is_none()
    {
        return None;
    }
    Some(etag)
}

/// Resolves an HTTP `304 Not Modified` response using cached release metadata.
pub fn resolve_check_outcome_from_304(
    settings: &mut UpdateSettings,
    current_version: &str,
    now_epoch_secs: u64,
) -> Result<UpdateCheckState, UpdateError> {
    let Some(cached) = settings.cached_latest_release.clone() else {
        return Err(UpdateError::InvalidJson(
            "304 Not Modified received without cached release metadata".to_string(),
        ));
    };
    if validate_release_url(&cached.html_url).is_err()
        || parse_semver_core(&cached.tag_name).is_none()
    {
        settings.cached_latest_release = None;
        settings.etag = None;
        return Err(UpdateError::InvalidUrl(
            "Cached release metadata failed validation".to_string(),
        ));
    }

    settings.last_checked_epoch_secs = Some(now_epoch_secs);
    Ok(classify_release_against_current(cached, current_version))
}

fn classify_release_against_current(
    release: ReleaseInfo,
    current_version: &str,
) -> UpdateCheckState {
    if is_newer_version(&release.tag_name, current_version) {
        UpdateCheckState::UpdateAvailable(release)
    } else {
        UpdateCheckState::UpToDate {
            checked_version: current_version.to_string(),
            release: Some(release),
        }
    }
}

fn sanitize_assets(assets: Vec<ReleaseAsset>) -> Vec<ReleaseAsset> {
    assets
        .into_iter()
        .filter(|asset| {
            !asset.name.is_empty()
                && !asset.name.contains(['/', '\\', '\0'])
                && !asset.name.chars().any(char::is_control)
                && validate_release_url(&asset.browser_download_url).is_ok()
        })
        .collect()
}

fn sanitize_published_at(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parses and validates a GitHub `/releases/latest` JSON payload within the 256 KiB cap.
pub fn parse_github_release_json(bytes: &[u8]) -> Result<ReleaseInfo, UpdateError> {
    if bytes.len() > MAX_GITHUB_RESPONSE_BYTES {
        return Err(UpdateError::ResponseTooLarge);
    }
    let payload: GithubLatestReleasePayload =
        serde_json::from_slice(bytes).map_err(|e| UpdateError::InvalidJson(e.to_string()))?;

    if payload.draft || payload.prerelease {
        return Err(UpdateError::DraftOrPrerelease);
    }
    if parse_semver_core(&payload.tag_name).is_none() {
        return Err(UpdateError::InvalidVersion(payload.tag_name));
    }
    let html_url = validate_release_url(&payload.html_url)?;
    let body = payload.body.as_deref().and_then(sanitize_release_notes);
    let published_at = sanitize_published_at(payload.published_at.as_deref());
    let assets = sanitize_assets(payload.assets);

    Ok(ReleaseInfo {
        tag_name: payload.tag_name,
        html_url,
        body,
        published_at,
        assets,
    })
}

/// Parses a single-hop redirect `Location` header from `GITHUB_WEB_LATEST_URL`.
pub fn parse_redirect_location_release(location: &str) -> Result<ReleaseInfo, UpdateError> {
    let trimmed = location.trim();
    if trimmed.is_empty() {
        return Err(UpdateError::MissingRedirectLocation);
    }

    let candidate = if trimmed.starts_with('/') {
        format!("https://github.com{trimmed}")
    } else {
        trimmed.to_string()
    };

    let validated_url = validate_release_url(&candidate)?;
    let tag_prefix = "https://github.com/hwanjongyu/LogAnalysis/releases/tag/";
    let Some(raw_tag_part) = validated_url.strip_prefix(tag_prefix) else {
        return Err(UpdateError::InvalidUrl(
            "Fallback redirect did not point to a release tag URL".to_string(),
        ));
    };
    let raw_tag = raw_tag_part
        .split(['?', '#'])
        .next()
        .unwrap_or(raw_tag_part);
    if raw_tag.is_empty() || raw_tag.contains('/') {
        return Err(UpdateError::InvalidVersion(raw_tag.to_string()));
    }
    if parse_semver_core(raw_tag).is_none() {
        return Err(UpdateError::InvalidVersion(raw_tag.to_string()));
    }

    Ok(ReleaseInfo {
        tag_name: raw_tag.to_string(),
        html_url: validated_url,
        body: None,
        published_at: None,
        assets: Vec::new(),
    })
}

/// Stage 2 seam: maps `(os, arch, is_appimage)` to the release asset filename in `release.yml`.
pub fn platform_asset_name(os: &str, arch: &str, is_appimage: bool) -> Option<&'static str> {
    match (os, arch, is_appimage) {
        ("linux", "x86_64", true) => Some("LogAnalysis-x86_64.AppImage"),
        ("linux", "x86_64", false) => Some("LogAnalysis-linux-x86_64.tar.gz"),
        ("macos", "aarch64", _) => Some("LogAnalysis-macos-aarch64.zip"),
        ("macos", "x86_64", _) => Some("LogAnalysis-macos-x86_64.zip"),
        ("windows", "x86_64", _) => Some("LogAnalysis-windows-x86_64.zip"),
        _ => None,
    }
}

/// Stage 2 seam: parses a `SHA256SUMS.txt` file for the 32-byte SHA-256 digest of `asset_name`.
pub fn parse_sha256_sums(contents: &str, asset_name: &str) -> Option<[u8; 32]> {
    if asset_name.is_empty() {
        return None;
    }
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let hex_digest = parts.next()?.trim();
        let file_part = parts.next()?.trim_start();
        let file_name = file_part
            .strip_prefix('*')
            .unwrap_or(file_part)
            .trim();
        let normalized_file = file_name.strip_prefix("./").unwrap_or(file_name);
        if normalized_file != asset_name {
            continue;
        }
        return decode_sha256_hex(hex_digest);
    }
    None
}

fn decode_sha256_hex(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Monotonically advances the single-flight check generation counter.
#[inline]
pub fn next_update_check_gen(gen: &mut u64) -> u64 {
    *gen = gen.wrapping_add(1);
    *gen
}

/// Returns `true` when `expected_gen` still matches the active check generation.
#[inline]
pub fn is_update_check_current(current_gen: u64, expected_gen: u64) -> bool {
    current_gen == expected_gen
}

/// Returns the current UNIX timestamp in seconds.
pub fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolves the platform-appropriate path to `update_settings.json`.
pub fn update_settings_path() -> Option<PathBuf> {
    config_file_path("update_settings.json")
}

/// Loads `update_settings.json` with a 64 KiB read cap and corrupt/poisoned cache recovery.
pub fn load_update_settings_from_path(path: &Path) -> UpdateSettings {
    let Ok(data) = read_to_string_bounded(path, MAX_UPDATE_SETTINGS_BYTES) else {
        return UpdateSettings::default();
    };
    let Ok(mut settings) = serde_json::from_str::<UpdateSettings>(&data) else {
        return UpdateSettings::default();
    };

    if settings.version == 0 {
        settings.version = UPDATE_SETTINGS_VERSION;
    }
    if let Some(etag) = settings.etag.as_deref() {
        let clean_etag = etag.trim();
        if clean_etag.is_empty() || clean_etag.chars().any(char::is_control) {
            settings.etag = None;
        }
    }
    if let Some(cached) = settings.cached_latest_release.as_mut() {
        if validate_release_url(&cached.html_url).is_err()
            || parse_semver_core(&cached.tag_name).is_none()
        {
            settings.cached_latest_release = None;
            settings.etag = None;
        } else {
            cached.body = cached.body.as_deref().and_then(sanitize_release_notes);
            cached.published_at = sanitize_published_at(cached.published_at.as_deref());
            cached.assets = sanitize_assets(std::mem::take(&mut cached.assets));
        }
    }

    settings
}

/// Atomically writes `settings` as pretty JSON to `path`.
pub fn save_update_settings_to_path(path: &Path, settings: &UpdateSettings) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    write_file_atomically(path, &bytes)
}

fn save_update_settings_serialized_with(
    snapshot: &UpdateSettings,
    path: &Path,
    save_gen: u64,
    lock: &Mutex<()>,
    current_gen: &AtomicU64,
) -> std::io::Result<bool> {
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if current_gen.load(Ordering::SeqCst) != save_gen {
        return Ok(false);
    }
    save_update_settings_to_path(path, snapshot)?;
    Ok(true)
}

fn save_update_settings_serialized(snapshot: &UpdateSettings, save_gen: u64) {
    let Some(path) = update_settings_path() else {
        return;
    };
    if let Err(err) = save_update_settings_serialized_with(
        snapshot,
        &path,
        save_gen,
        &UPDATE_SETTINGS_SAVE_LOCK,
        &UPDATE_SETTINGS_SAVE_GEN,
    ) {
        crate::logging::log_event(
            "UPDATE",
            &format!("Failed to persist update_settings.json: {err}"),
        );
    }
}

fn build_ureq_agent(timeout: Duration, max_redirects: u32) -> Result<ureq::Agent, UpdateError> {
    let cert_result = rustls_native_certs::load_native_certs();
    let certs: Vec<ureq::tls::Certificate<'static>> = cert_result
        .certs
        .into_iter()
        .map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned())
        .collect();
    if certs.is_empty() {
        return Err(UpdateError::TlsUnavailable);
    }

    let crypto = Arc::new(rustls::crypto::ring::default_provider());
    let tls_config = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::Specific(Arc::new(certs)))
        .unversioned_rustls_crypto_provider(crypto)
        .build();

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .https_only(true)
        .max_redirects(max_redirects)
        .tls_config(tls_config)
        .build();

    Ok(config.into())
}

fn sanitize_header_value(raw: Option<&str>, max_len: usize) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_bounded_body(
    response: &mut ureq::http::Response<ureq::Body>,
    max_bytes: usize,
) -> Result<Vec<u8>, UpdateError> {
    let mut buf = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    if buf.len() > max_bytes {
        return Err(UpdateError::ResponseTooLarge);
    }
    Ok(buf)
}

fn fetch_fallback_redirect_release(
    timeout: Duration,
    user_agent: &str,
) -> Result<ReleaseInfo, UpdateError> {
    let agent = build_ureq_agent(timeout, 0)?;

    let head_resp = agent
        .head(GITHUB_WEB_LATEST_URL)
        .header("User-Agent", user_agent)
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let head_status = head_resp.status().as_u16();
    let head_location = head_resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if matches!(head_status, 301 | 302 | 303 | 307 | 308) {
        if let Some(loc) = head_location {
            return parse_redirect_location_release(&loc);
        }
    }

    if head_status != 405 && head_location.is_some() {
        return Err(UpdateError::HttpStatus(head_status));
    }

    let mut get_resp = agent
        .get(GITHUB_WEB_LATEST_URL)
        .header("User-Agent", user_agent)
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let get_status = get_resp.status().as_u16();
    let get_location = get_resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    // Drain at most 16 KiB cleanly before dropping the body.
    let mut drain_buf = [0u8; 4096];
    let mut reader = get_resp
        .body_mut()
        .as_reader()
        .take(MAX_FALLBACK_BODY_DRAIN_BYTES as u64);
    while matches!(reader.read(&mut drain_buf), Ok(n) if n > 0) {}

    if !matches!(get_status, 301 | 302 | 303 | 307 | 308) {
        return Err(UpdateError::HttpStatus(get_status));
    }
    let Some(loc) = get_location else {
        return Err(UpdateError::MissingRedirectLocation);
    };
    parse_redirect_location_release(&loc)
}

/// Applies the result of a fallback release lookup while preserving richer cached API data/ETag.
pub fn apply_fallback_release_to_settings(
    settings: &mut UpdateSettings,
    fallback_release: ReleaseInfo,
    current_version: &str,
    now_epoch_secs: u64,
) -> UpdateCheckState {
    settings.last_checked_epoch_secs = Some(now_epoch_secs);

    let effective_release = match settings.cached_latest_release.as_ref() {
        Some(existing)
            if parse_semver_core(&existing.tag_name)
                == parse_semver_core(&fallback_release.tag_name)
                && (existing.body.is_some() || !existing.assets.is_empty()) =>
        {
            existing.clone()
        }
        _ => {
            settings.cached_latest_release = Some(fallback_release.clone());
            fallback_release
        }
    };

    classify_release_against_current(effective_release, current_version)
}

/// Executes a blocking GitHub latest-release check (must run inside a background executor).
pub fn check_github_latest_release(
    mut settings: UpdateSettings,
    current_version: &str,
    timeout: Duration,
    now_epoch_secs: u64,
) -> Result<(UpdateSettings, UpdateCheckState), UpdateError> {
    let user_agent = format!("LogAnalysis/{current_version}");
    let agent = build_ureq_agent(timeout, 5)?;

    let mut req = agent
        .get(GITHUB_API_LATEST_URL)
        .header("User-Agent", &user_agent)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(etag) = build_conditional_etag(&settings) {
        req = req.header("If-None-Match", etag);
    }

    let mut response = req.call().map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = response.status().as_u16();

    match status {
        200 => {
            let new_etag = sanitize_header_value(
                response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok()),
                256,
            );
            let bytes = read_bounded_body(&mut response, MAX_GITHUB_RESPONSE_BYTES)?;
            let release = parse_github_release_json(&bytes)?;
            settings.last_checked_epoch_secs = Some(now_epoch_secs);
            settings.etag = new_etag;
            settings.cached_latest_release = Some(release.clone());
            let state = classify_release_against_current(release, current_version);
            Ok((settings, state))
        }
        304 => {
            let state =
                resolve_check_outcome_from_304(&mut settings, current_version, now_epoch_secs)?;
            Ok((settings, state))
        }
        403 | 429 => {
            let fallback_release = fetch_fallback_redirect_release(timeout, &user_agent)?;
            let state = apply_fallback_release_to_settings(
                &mut settings,
                fallback_release,
                current_version,
                now_epoch_secs,
            );
            Ok((settings, state))
        }
        code => Err(UpdateError::HttpStatus(code)),
    }
}

impl LogAnalysisWorkspace {
    /// Loads `update_settings.json` asynchronously and performs a throttled silent startup check.
    pub fn load_update_settings_and_maybe_check_async(&mut self, cx: &mut Context<Self>) {
        let bg_executor = cx.background_executor().clone();
        let Some(settings_path) = update_settings_path() else {
            self.update_settings_loaded = true;
            return;
        };

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let loaded = bg_executor
                .spawn(async move { load_update_settings_from_path(&settings_path) })
                .await;

            let now_secs = current_epoch_secs();
            let mut next_check: Option<(u64, UpdateSettings)> = None;

            let _ = this.update(cx, |workspace, cx| {
                workspace.update_settings = loaded.clone();
                workspace.update_settings_loaded = true;

                if let Some(cached) = loaded.cached_latest_release.clone() {
                    if is_newer_version(&cached.tag_name, env!("CARGO_PKG_VERSION")) {
                        workspace.update_check_state = UpdateCheckState::UpdateAvailable(cached);
                    }
                }

                if should_run_auto_check(&workspace.update_settings, now_secs) {
                    let check_gen = next_update_check_gen(&mut workspace.update_check_gen);
                    next_check = Some((check_gen, workspace.update_settings.clone()));
                }

                cx.notify();
            });

            let Some((check_gen, settings_snapshot)) = next_check else {
                return;
            };

            let check_result = bg_executor
                .spawn(async move {
                    let res = check_github_latest_release(
                        settings_snapshot,
                        env!("CARGO_PKG_VERSION"),
                        Duration::from_secs(STARTUP_CHECK_TIMEOUT_SECS),
                        current_epoch_secs(),
                    );
                    if let Ok((ref updated_settings, _)) = res {
                        let save_gen =
                            UPDATE_SETTINGS_SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
                        save_update_settings_serialized(updated_settings, save_gen);
                    }
                    res
                })
                .await;

            let _ = this.update(cx, |workspace, cx| {
                if !is_update_check_current(workspace.update_check_gen, check_gen) {
                    return;
                }
                match check_result {
                    Ok((updated_settings, new_state)) => {
                        workspace.update_settings = updated_settings;
                        workspace.update_check_state = new_state;
                        cx.notify();
                    }
                    Err(err) => {
                        // Silent background failure rule: never surface error modal/state on startup.
                        crate::logging::log_event(
                            "UPDATE",
                            &format!("Background update check skipped: {err}"),
                        );
                    }
                }
            });
        })
        .detach();
    }

    /// Triggers an immediate manual update check, bypassing the 24-hour throttle.
    pub fn check_for_updates_manual(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update_check_state, UpdateCheckState::Checking) {
            // Coalesce if a manual check is already active; only bump generation if needed.
        }
        let check_gen = next_update_check_gen(&mut self.update_check_gen);
        let settings_snapshot = self.update_settings.clone();
        self.update_check_state = UpdateCheckState::Checking;
        cx.notify();

        let bg_executor = cx.background_executor().clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let check_result = bg_executor
                .spawn(async move {
                    let res = check_github_latest_release(
                        settings_snapshot,
                        env!("CARGO_PKG_VERSION"),
                        Duration::from_secs(MANUAL_CHECK_TIMEOUT_SECS),
                        current_epoch_secs(),
                    );
                    if let Ok((ref updated_settings, _)) = res {
                        let save_gen =
                            UPDATE_SETTINGS_SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
                        save_update_settings_serialized(updated_settings, save_gen);
                    }
                    res
                })
                .await;

            let _ = this.update(cx, |workspace, cx| {
                if !is_update_check_current(workspace.update_check_gen, check_gen) {
                    return;
                }
                match check_result {
                    Ok((updated_settings, new_state)) => {
                        workspace.update_settings = updated_settings;
                        workspace.update_check_state = new_state;
                    }
                    Err(err) => {
                        crate::logging::log_event(
                            "UPDATE",
                            &format!("Manual update check failed: {err}"),
                        );
                        workspace.update_check_state = UpdateCheckState::Error(err.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn prepare_update_modal_open(&mut self, cx: &mut Context<Self>) {
        self.is_overflow_menu_open = false;
        self.adb.is_device_selector_open = false;
        self.is_command_palette_open = false;
        self.bookmarks.is_drawer_open = false;
        self.bookmarks.is_note_modal_open = false;
        if self.filter_edit_modal.is_open {
            self.close_filter_modal(cx);
        }
        self.input_focus = InputFocusTarget::None;
        self.is_update_modal_open = true;
        self.update_notes_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
    }

    /// Opens the Software Update modal, dismissing mutually exclusive popovers/modals.
    pub fn open_update_modal(&mut self, cx: &mut Context<Self>) {
        self.prepare_update_modal_open(cx);
        if matches!(self.update_check_state, UpdateCheckState::Idle) {
            self.check_for_updates_manual(cx);
        } else {
            cx.notify();
        }
    }

    /// Opens the Software Update modal and immediately runs a manual GitHub check.
    pub fn open_update_modal_and_check(&mut self, cx: &mut Context<Self>) {
        self.prepare_update_modal_open(cx);
        self.check_for_updates_manual(cx);
    }

    /// Closes the Software Update modal.
    pub fn close_update_modal(&mut self, cx: &mut Context<Self>) {
        if self.is_update_modal_open {
            self.is_update_modal_open = false;
            cx.notify();
        }
    }

    /// Toggles automatic startup update checks and persists `update_settings.json` asynchronously.
    pub fn toggle_auto_check_updates(&mut self, cx: &mut Context<Self>) {
        self.update_settings.auto_check_updates = !self.update_settings.auto_check_updates;
        let enabled = self.update_settings.auto_check_updates;
        self.status_text = if enabled {
            "Auto-check for updates on startup: ON".into()
        } else {
            "Auto-check for updates on startup: OFF".into()
        };
        self.persist_update_settings_async(cx);
        cx.notify();
    }

    /// Marks the currently available release version as skipped and persists the preference.
    pub fn skip_available_update_version(&mut self, cx: &mut Context<Self>) {
        let tag = match &self.update_check_state {
            UpdateCheckState::UpdateAvailable(info) => Some(info.tag_name.clone()),
            _ => None,
        };
        let Some(tag) = tag else {
            return;
        };
        self.update_settings.skipped_version = Some(tag.clone());
        let display = format_display_tag(&tag);
        self.status_text = format!("Skipped update notification for {display}").into();
        self.persist_update_settings_async(cx);
        self.is_update_modal_open = false;
        cx.notify();
    }

    /// Validates the release URL against the strict GitHub allowlist and opens it in the OS browser.
    pub fn open_verified_release_page(&mut self, cx: &mut Context<Self>) {
        let candidate_url = match &self.update_check_state {
            UpdateCheckState::UpdateAvailable(info) => Some(info.html_url.clone()),
            UpdateCheckState::UpToDate {
                release: Some(info),
                ..
            } => Some(info.html_url.clone()),
            _ => self
                .update_settings
                .cached_latest_release
                .as_ref()
                .map(|r| r.html_url.clone()),
        };

        let Some(raw_url) = candidate_url else {
            return;
        };

        let validated_url = match validate_release_url(&raw_url) {
            Ok(u) => u,
            Err(err) => {
                crate::logging::log_event("UPDATE", &err.to_string());
                self.update_check_state = UpdateCheckState::Error(err.to_string());
                cx.notify();
                return;
            }
        };

        let bg_executor = cx.background_executor().clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let launch_res = bg_executor
                .spawn(async move { open::that_detached(&validated_url) })
                .await;

            if let Err(err) = launch_res {
                let msg = UpdateError::BrowserLaunch(err.to_string()).to_string();
                crate::logging::log_event("UPDATE", &msg);
                let _ = this.update(cx, |workspace, cx| {
                    workspace.status_text = msg.clone().into();
                    workspace.update_check_state = UpdateCheckState::Error(msg);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn persist_update_settings_async(&self, cx: &mut Context<Self>) {
        let snapshot = self.update_settings.clone();
        let save_gen = UPDATE_SETTINGS_SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        let bg_executor = cx.background_executor().clone();
        bg_executor
            .spawn(async move {
                save_update_settings_serialized(&snapshot, save_gen);
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    // Explicit imports: `super::*` would pull in `gpui::test`, shadowing the std `#[test]` attribute.
    use super::{
        apply_fallback_release_to_settings, build_conditional_etag, format_published_date,
        is_newer_version, is_update_check_current, load_update_settings_from_path,
        next_update_check_gen, parse_github_release_json, parse_redirect_location_release,
        parse_semver_core, parse_sha256_sums, platform_asset_name,
        resolve_check_outcome_from_304, sanitize_release_notes,
        save_update_settings_serialized_with, save_update_settings_to_path, should_run_auto_check,
        validate_release_url, ReleaseInfo, SemVerCore, UpdateCheckState, UpdateError,
        UpdateSettings, MAX_GITHUB_RESPONSE_BYTES, MAX_RELEASE_NOTES_CHARS,
        MAX_UPDATE_SETTINGS_BYTES, UPDATE_CHECK_INTERVAL_SECS, UPDATE_SETTINGS_VERSION,
    };
    use std::sync::atomic::AtomicU64;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[test]
    fn test_parse_semver_core_and_is_newer_version() {
        assert_eq!(parse_semver_core("0.2.5"), Some(SemVerCore(0, 2, 5)));
        assert_eq!(parse_semver_core("v0.2.6"), Some(SemVerCore(0, 2, 6)));
        assert_eq!(parse_semver_core("V1.2.3"), Some(SemVerCore(1, 2, 3)));
        assert_eq!(
            parse_semver_core("v0.2.6+build.42"),
            Some(SemVerCore(0, 2, 6))
        );
        assert_eq!(parse_semver_core("0.2.10"), Some(SemVerCore(0, 2, 10)));

        // Comparisons
        assert!(is_newer_version("v0.2.6", "0.2.5"));
        assert!(is_newer_version("V0.2.10", "0.2.9"));
        assert!(is_newer_version("1.0.0", "0.99.99"));
        assert!(!is_newer_version("V0.2.5", "0.2.5"));
        assert!(!is_newer_version("v0.2.4", "0.2.5"));
        assert!(!is_newer_version("v0.2.5+build.9", "0.2.5"));

        // Rejections: pre-releases, blank, non-numeric, 2-part, whitespace, invalid builds
        for bad in [
            "",
            "   ",
            "v0.3.0-rc.1",
            "v0.3.0-alpha",
            "0.2",
            "0.2.5.1",
            "v0.2.a",
            " v0.2.6",
            "v0.2.6 ",
            "vv0.2.6",
            "v0.2.6+",
            "+0.2.6",
        ] {
            assert_eq!(parse_semver_core(bad), None, "expected None for {bad:?}");
            assert!(!is_newer_version(bad, "0.2.5"));
        }
    }

    #[test]
    fn test_is_version_skipped_normalizes_leading_v() {
        let mut settings = UpdateSettings {
            skipped_version: Some("v0.2.6".to_string()),
            ..Default::default()
        };
        assert!(settings.is_version_skipped("v0.2.6"));
        assert!(settings.is_version_skipped("V0.2.6"));
        assert!(settings.is_version_skipped("0.2.6"));
        assert!(!settings.is_version_skipped("v0.2.7"));

        settings.skipped_version = Some("0.2.7".to_string());
        assert!(settings.is_version_skipped("v0.2.7"));
        assert!(settings.is_version_skipped("V0.2.7"));

        settings.skipped_version = None;
        assert!(!settings.is_version_skipped("v0.2.6"));
    }

    #[test]
    fn test_should_run_auto_check_24h_throttle_and_clock_skew() {
        let mut settings = UpdateSettings::default();
        assert!(should_run_auto_check(&settings, 1_000_000));

        settings.auto_check_updates = false;
        assert!(!should_run_auto_check(&settings, 1_000_000));

        settings.auto_check_updates = true;
        settings.last_checked_epoch_secs = Some(1_000_000);
        // 86_399s elapsed -> throttled
        assert!(!should_run_auto_check(
            &settings,
            1_000_000 + UPDATE_CHECK_INTERVAL_SECS - 1
        ));
        // Exactly 86_400s elapsed -> allowed
        assert!(should_run_auto_check(
            &settings,
            1_000_000 + UPDATE_CHECK_INTERVAL_SECS
        ));
        // Clock skew (last > now) -> saturating_sub = 0 -> throttled safely
        assert!(!should_run_auto_check(&settings, 999_000));
    }

    #[test]
    fn test_build_conditional_etag_and_304_resolution() {
        let valid_release = ReleaseInfo {
            tag_name: "v0.2.6".to_string(),
            html_url: "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6".to_string(),
            body: Some("Bug fixes".to_string()),
            published_at: Some("2026-11-20T00:00:00Z".to_string()),
            assets: Vec::new(),
        };

        // Missing cached_latest_release -> must NOT send If-None-Match
        let mut settings = UpdateSettings {
            etag: Some("\"abc\"".to_string()),
            cached_latest_release: None,
            ..Default::default()
        };
        assert_eq!(build_conditional_etag(&settings), None);
        assert!(resolve_check_outcome_from_304(&mut settings, "0.2.5", 2000).is_err());

        // Valid etag + cached_latest_release -> emits ETag and resolves 304 to UpdateAvailable
        settings.cached_latest_release = Some(valid_release.clone());
        assert_eq!(build_conditional_etag(&settings), Some("\"abc\""));
        let outcome = resolve_check_outcome_from_304(&mut settings, "0.2.5", 5000).unwrap();
        assert_eq!(settings.last_checked_epoch_secs, Some(5000));
        assert_eq!(outcome, UpdateCheckState::UpdateAvailable(valid_release));

        // Same cached tag when running 0.2.6 -> resolves 304 to UpToDate
        let outcome_up_to_date =
            resolve_check_outcome_from_304(&mut settings, "0.2.6", 6000).unwrap();
        assert_eq!(settings.last_checked_epoch_secs, Some(6000));
        assert!(matches!(
            outcome_up_to_date,
            UpdateCheckState::UpToDate {
                checked_version,
                release: Some(_)
            } if checked_version == "0.2.6"
        ));
    }

    #[test]
    fn test_parse_github_release_json_valid_draft_prerelease_and_oversized() {
        let valid_json = br###"{
            "tag_name": "v0.2.6",
            "html_url": "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6",
            "body": "## Changes\r\n- Fast filtering\tOK",
            "published_at": "2026-11-20T12:34:56Z",
            "draft": false,
            "prerelease": false,
            "assets": [
                {
                    "name": "LogAnalysis-x86_64.AppImage",
                    "browser_download_url": "https://github.com/hwanjongyu/LogAnalysis/releases/download/v0.2.6/LogAnalysis-x86_64.AppImage",
                    "size": 123456
                },
                {
                    "name": "spoofed.zip",
                    "browser_download_url": "https://evil.com/spoofed.zip",
                    "size": 99
                }
            ]
        }"###;

        let parsed = parse_github_release_json(valid_json).unwrap();
        assert_eq!(parsed.tag_name, "v0.2.6");
        assert_eq!(
            parsed.html_url,
            "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6"
        );
        assert_eq!(
            parsed.body.as_deref(),
            Some("## Changes\n- Fast filtering  OK")
        );
        assert_eq!(
            format_published_date(parsed.published_at.as_deref()),
            Some("2026-11-20".to_string())
        );
        // Only allowlisted asset URLs are retained
        assert_eq!(parsed.assets.len(), 1);
        assert_eq!(parsed.assets[0].name, "LogAnalysis-x86_64.AppImage");

        // Reject draft and prerelease
        let draft_json = br#"{"tag_name":"v0.2.6","html_url":"https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6","draft":true,"prerelease":false}"#;
        assert_eq!(
            parse_github_release_json(draft_json),
            Err(UpdateError::DraftOrPrerelease)
        );
        let pre_json = br#"{"tag_name":"v0.3.0-rc.1","html_url":"https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.3.0-rc.1","draft":false,"prerelease":true}"#;
        assert_eq!(
            parse_github_release_json(pre_json),
            Err(UpdateError::DraftOrPrerelease)
        );

        // Reject > 256 KiB payload
        let oversized = vec![b' '; MAX_GITHUB_RESPONSE_BYTES + 1];
        assert_eq!(
            parse_github_release_json(&oversized),
            Err(UpdateError::ResponseTooLarge)
        );
    }

    #[test]
    fn test_parse_redirect_location_release_and_fallback_settings() {
        let abs = parse_redirect_location_release(
            "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6",
        )
        .unwrap();
        assert_eq!(abs.tag_name, "v0.2.6");
        assert_eq!(abs.body, None);

        let rel = parse_redirect_location_release("/hwanjongyu/LogAnalysis/releases/tag/v0.2.7")
            .unwrap();
        assert_eq!(rel.tag_name, "v0.2.7");
        assert_eq!(
            rel.html_url,
            "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.7"
        );

        // Spoofed or invalid redirect targets rejected
        assert!(parse_redirect_location_release(
            "https://evil.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6"
        )
        .is_err());
        assert!(parse_redirect_location_release(
            "/hwanjongyu/LogAnalysis/releases/tag/v0.3.0-rc.1"
        )
        .is_err());
        assert!(parse_redirect_location_release("").is_err());

        // Richer cached API ReleaseInfo is preserved on fallback for the same tag
        let mut settings = UpdateSettings {
            etag: Some("\"api-etag\"".to_string()),
            cached_latest_release: Some(ReleaseInfo {
                tag_name: "v0.2.6".to_string(),
                html_url: "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6"
                    .to_string(),
                body: Some("Rich notes".to_string()),
                published_at: Some("2026-11-20T00:00:00Z".to_string()),
                assets: Vec::new(),
            }),
            ..Default::default()
        };
        let state = apply_fallback_release_to_settings(&mut settings, abs, "0.2.5", 9000);
        assert_eq!(settings.etag.as_deref(), Some("\"api-etag\""));
        assert_eq!(settings.last_checked_epoch_secs, Some(9000));
        match state {
            UpdateCheckState::UpdateAvailable(info) => {
                assert_eq!(info.body.as_deref(), Some("Rich notes"));
            }
            other => panic!("expected UpdateAvailable, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_release_url_allowlist() {
        assert!(
            validate_release_url("https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6")
                .is_ok()
        );
        assert!(validate_release_url("https://github.com/hwanjongyu/LogAnalysis/releases").is_ok());

        for bad in [
            "",
            "http://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://evil.com/hwanjongyu/LogAnalysis/releases",
            "https://github.com/other/repo/releases",
            "https://user@github.com/hwanjongyu/LogAnalysis/releases",
            "https://github.com:8443/hwanjongyu/LogAnalysis/releases",
            "https://github.com/hwanjongyu/LogAnalysis/releases_evil",
            "https://github.com/hwanjongyu/LogAnalysis/releases/../other",
            "https://github.com/hwanjongyu/LogAnalysis/releases/./tag/v0.2.6",
            "https://github.com/hwanjongyu/LogAnalysis/releases\\tag",
            " https://github.com/hwanjongyu/LogAnalysis/releases",
        ] {
            assert!(
                validate_release_url(bad).is_err(),
                "expected validate_release_url to reject {bad:?}"
            );
        }
    }

    #[test]
    fn test_sanitize_release_notes_control_bidi_and_multibyte_truncation() {
        let raw = "Hello\r\n\r\n\r\n\r\n\r\nWorld\u{202E}!\x07\tEnd";
        assert_eq!(
            sanitize_release_notes(raw).as_deref(),
            Some("Hello\n\nWorld!  End")
        );
        assert_eq!(sanitize_release_notes("   \r\n\t\x00  "), None);

        // Multi-byte UTF-8 boundary check (CJK + emoji > 4000 chars)
        let long_cjk: String = "한🚀".repeat(2500); // 5000 chars
        let sanitized = sanitize_release_notes(&long_cjk).expect("non-empty");
        assert!(sanitized.ends_with("\n… (view full release notes on GitHub)"));
        let prefix = sanitized
            .strip_suffix("\n… (view full release notes on GitHub)")
            .unwrap();
        assert_eq!(prefix.chars().count(), MAX_RELEASE_NOTES_CHARS);
    }

    #[test]
    fn test_update_settings_persistence_corrupt_oversized_poisoned_and_gen_guard() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update_settings.json");

        // Missing file -> default (auto_check_updates = true)
        let missing = load_update_settings_from_path(&path);
        assert_eq!(missing, UpdateSettings::default());
        assert!(missing.auto_check_updates);

        // Valid round-trip
        let settings = UpdateSettings {
            version: UPDATE_SETTINGS_VERSION,
            auto_check_updates: false,
            last_checked_epoch_secs: Some(123_456),
            etag: Some("\"etag-1\"".to_string()),
            cached_latest_release: Some(ReleaseInfo {
                tag_name: "v0.2.6".to_string(),
                html_url: "https://github.com/hwanjongyu/LogAnalysis/releases/tag/v0.2.6"
                    .to_string(),
                body: Some("Line 1\r\nLine 2".to_string()),
                published_at: Some("2026-11-20T00:00:00Z".to_string()),
                assets: Vec::new(),
            }),
            skipped_version: Some("v0.2.6".to_string()),
        };
        save_update_settings_to_path(&path, &settings).unwrap();
        let loaded = load_update_settings_from_path(&path);
        assert!(!loaded.auto_check_updates);
        assert_eq!(loaded.last_checked_epoch_secs, Some(123_456));
        assert_eq!(
            loaded.cached_latest_release.as_ref().unwrap().body.as_deref(),
            Some("Line 1\nLine 2")
        );

        // Corrupt JSON -> default
        std::fs::write(&path, "{not valid json").unwrap();
        assert_eq!(load_update_settings_from_path(&path), UpdateSettings::default());

        // Oversized (> 64 KiB) -> default
        let oversized = vec![b' '; MAX_UPDATE_SETTINGS_BYTES + 1];
        std::fs::write(&path, oversized).unwrap();
        assert_eq!(load_update_settings_from_path(&path), UpdateSettings::default());

        // Poisoned cached html_url -> cache + etag discarded while user flags survive
        let poisoned = UpdateSettings {
            auto_check_updates: false,
            etag: Some("\"stale\"".to_string()),
            cached_latest_release: Some(ReleaseInfo {
                tag_name: "v0.2.6".to_string(),
                html_url: "https://evil.com/phish".to_string(),
                body: Some("Evil".to_string()),
                published_at: None,
                assets: Vec::new(),
            }),
            ..Default::default()
        };
        save_update_settings_to_path(&path, &poisoned).unwrap();
        let sanitized = load_update_settings_from_path(&path);
        assert!(!sanitized.auto_check_updates);
        assert_eq!(sanitized.cached_latest_release, None);
        assert_eq!(sanitized.etag, None);

        // Serialized save generation guard skips stale save generations
        let lock = Mutex::new(());
        let current_gen = AtomicU64::new(2);
        let stale = save_update_settings_serialized_with(
            &settings,
            &path,
            1,
            &lock,
            &current_gen,
        )
        .unwrap();
        assert!(!stale);

        let fresh = save_update_settings_serialized_with(
            &settings,
            &path,
            2,
            &lock,
            &current_gen,
        )
        .unwrap();
        assert!(fresh);
    }

    #[test]
    fn test_generation_guard_and_stage2_seams() {
        let mut gen = 0;
        let g1 = next_update_check_gen(&mut gen);
        assert_eq!(g1, 1);
        assert!(is_update_check_current(gen, g1));

        let g2 = next_update_check_gen(&mut gen);
        assert_eq!(g2, 2);
        assert!(!is_update_check_current(gen, g1));
        assert!(is_update_check_current(gen, g2));

        // Stage 2 platform asset names matching release.yml
        assert_eq!(
            platform_asset_name("linux", "x86_64", true),
            Some("LogAnalysis-x86_64.AppImage")
        );
        assert_eq!(
            platform_asset_name("linux", "x86_64", false),
            Some("LogAnalysis-linux-x86_64.tar.gz")
        );
        assert_eq!(
            platform_asset_name("macos", "aarch64", false),
            Some("LogAnalysis-macos-aarch64.zip")
        );
        assert_eq!(
            platform_asset_name("macos", "x86_64", false),
            Some("LogAnalysis-macos-x86_64.zip")
        );
        assert_eq!(
            platform_asset_name("windows", "x86_64", false),
            Some("LogAnalysis-windows-x86_64.zip")
        );
        assert_eq!(platform_asset_name("freebsd", "x86_64", false), None);

        // Stage 2 SHA256SUMS.txt parser
        let sums = "\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  LogAnalysis-linux-x86_64.tar.gz
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *./LogAnalysis-x86_64.AppImage
";
        let tar_hash = parse_sha256_sums(sums, "LogAnalysis-linux-x86_64.tar.gz").unwrap();
        assert_eq!(tar_hash[0], 0x01);
        assert_eq!(tar_hash[31], 0xef);
        let appimage_hash = parse_sha256_sums(sums, "LogAnalysis-x86_64.AppImage").unwrap();
        assert_eq!(appimage_hash[0], 0xfe);
        assert_eq!(appimage_hash[31], 0x10);
        assert_eq!(parse_sha256_sums(sums, "missing.zip"), None);
    }
}
