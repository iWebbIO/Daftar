//! Updating itself: the app asks GitHub what the newest version is, fetches the
//! APK and hands it to Android's own installer.
//!
//! The shopkeeper cannot be asked to download a file and open it. So the app
//! checks on its own, quietly, and when there is something newer it can say one
//! short line about it and install it with two taps from him — the rest is
//! Android's dialog, which is not something this app can or should skip.
//!
//! Three rules hold this module together:
//!
//! 1. **A check never fails loudly.** No network, a blocked GitHub, a rate
//!    limit — the answer is still `Ok`, with `behind: false` and one plain
//!    sentence. A shopkeeper must never see an error because a background
//!    check could not reach the internet.
//! 2. **A version is compared as numbers**, not as text: `1.0.10` is newer
//!    than `1.0.9`.
//! 3. **A half-download is never installable.** The bytes land in `.part` and
//!    are only renamed to `.apk` once the whole file is there.
//!
//! The network lives behind [`Github`] so the tests can be run with no
//! connection at all.

use crate::db::{self, Res};
use crate::Ctx;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
use serde_json::json;

/// GitHub's answer for this repository's newest release.
const RELEASE_API: &str = "https://api.github.com/repos/iWebbIO/Daftar/releases/latest";

/// GitHub refuses any request that does not name itself.
const AGENT: &str = "Daftar";

/// How long the small JSON call may take.
const ASK_WAIT: Duration = Duration::from_secs(15);
/// How long a stalled APK download may sit without a single byte arriving. The
/// total time is deliberately not capped: ten megabytes on a phone in a shop is
/// allowed to be slow, it is not allowed to hang.
const READ_WAIT: Duration = Duration::from_secs(30);

/// Where the last answer is kept, so a start-up check costs one row read.
const KEY_AT: &str = "update_checked_at";
const KEY_LATEST: &str = "update_latest";
const KEY_APK: &str = "update_apk_url";
/// A cached answer is reused for this long. GitHub counts requests per address,
/// and every start asks once.
const CACHE_FOR_MS: i64 = 6 * 60 * 60 * 1000;

/// Where the app stands against the newest release.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub behind: bool,
    pub apk_url: Option<String>,
    pub notes: String,
}

/// The APK that is now sitting in the app's files folder.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Downloaded {
    pub path: String,
    pub size: i64,
}

/// Talking to GitHub, behind a trait so no test ever touches the network.
pub trait Github {
    /// The newest release, as GitHub's JSON describes it.
    fn release(&self) -> Res<Value>;

    /// Streams one file into `to` and returns how many bytes arrived.
    fn fetch(&self, url: &str, to: &Path) -> Res<i64>;
}

/// The real one: two GETs over HTTPS.
pub struct Http;

impl Github for Http {
    fn release(&self) -> Res<Value> {
        let mut answer = agent(Some(ASK_WAIT), None)
            .get(RELEASE_API)
            .header("accept", "application/vnd.github+json")
            .header("user-agent", AGENT)
            .call()
            .map_err(net_error)?;

        let status = answer.status().as_u16();
        if status != 200 {
            return Err(refusal(status));
        }
        answer
            .body_mut()
            .read_json::<Value>()
            .map_err(|_| "پاسخ گیت‌هاب خوانده نشد".to_string())
    }

    fn fetch(&self, url: &str, to: &Path) -> Res<i64> {
        let answer = agent(None, Some(READ_WAIT))
            .get(url)
            .header("user-agent", AGENT)
            .call()
            .map_err(net_error)?;

        if answer.status().as_u16() != 200 {
            return Err("نسخه تازه دانلود نشد".to_string());
        }

        let mut file =
            std::fs::File::create(to).map_err(|_| "فایل نسخه تازه ساخته نشد".to_string())?;
        // A stream, not a buffer: the APK never sits in memory whole.
        let mut body = answer.into_body().into_reader();
        let copied =
            std::io::copy(&mut body, &mut file).map_err(|_| "نسخه تازه دانلود نشد".to_string())?;
        // `file` closes here, before the caller renames it.
        Ok(copied as i64)
    }
}

fn agent(everything: Option<Duration>, per_read: Option<Duration>) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        // A refusal is data here: the status code decides the sentence.
        .http_status_as_error(false)
        .timeout_global(everything)
        .timeout_per_read(per_read)
        .build();
    ureq::Agent::new_with_config(config)
}

/// No wording from the engine reaches the shopkeeper.
fn net_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_)
        | ureq::Error::Io(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound => "به اینترنت وصل نشد".to_string(),
        _ => "بررسی به‌روزرسانی نشد".to_string(),
    }
}

fn refusal(status: u16) -> String {
    match status {
        // GitHub answers 403 when the hourly limit is used up, and 404 while the
        // repository has no published release.
        403 | 429 => "گیت‌هاب فعلاً پاسخ نمی‌دهد".to_string(),
        404 => "نسخه تازه‌ای پیدا نشد".to_string(),
        _ => "بررسی به‌روزرسانی نشد".to_string(),
    }
}

// ------------------------------------------------------------------- the versions

/// The numbers inside a version string, so it can be compared as numbers.
///
/// `v1.0.10` → `[1, 0, 10]`, `0.1.1-beta` → `[0, 1, 1]`, `nightly` → `[0]`.
fn numbers(version: &str) -> Vec<u64> {
    let text = trim_v(version.trim());
    text.split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            // A part with no digits at all ("nightly", "beta") counts as zero,
            // which can never make an old version look new.
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

/// `v0.1.1` → `0.1.1`: the tag as the shopkeeper's version number.
fn trim_v(version: &str) -> &str {
    let trimmed = version.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
}

/// True when `a` is a later version than `b`.
///
/// Missing trailing parts count as zero, so `1.0` and `1.0.0` are the same
/// version and neither is newer — which is what a tag written by hand means.
/// A string with no digits at all in it is not a version, and comparing against
/// one claims nothing in either direction.
fn newer(a: &str, b: &str) -> bool {
    if !has_digits(a) || !has_digits(b) {
        return false;
    }
    let a = numbers(a);
    let b = numbers(b);
    let longest = a.len().max(b.len());
    for i in 0..longest {
        let left = a.get(i).copied().unwrap_or(0);
        let right = b.get(i).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

fn has_digits(version: &str) -> bool {
    version.chars().any(|c| c.is_ascii_digit())
}

/// The APK out of a release. Tauri publishes one universal APK per release; if
/// several ABIs are ever attached, the universal one is the right choice for a
/// phone we know nothing about.
fn pick_apk(release: &Value) -> Option<String> {
    let mut first = None;
    for asset in release.get("assets")?.as_array()? {
        let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".apk") {
            continue;
        }
        let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        if name.to_ascii_lowercase().contains("universal") {
            return Some(url.to_string());
        }
        if first.is_none() {
            first = Some(url.to_string());
        }
    }
    first
}

/// Works out where we stand from one GitHub answer. Pure, so it is easy to test.
pub fn evaluate(current: &str, release: &Value) -> UpdateInfo {
    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let latest = trim_v(tag).to_string();
    let behind = !latest.is_empty() && newer(&latest, current);
    UpdateInfo {
        current: current.to_string(),
        latest,
        behind,
        apk_url: pick_apk(release),
        notes: if behind {
            "نسخه تازه در دسترس است".to_string()
        } else {
            "برنامه به‌روز است".to_string()
        },
    }
}

/// The check itself, without a network of its own: a stub here in the tests.
pub fn check_with(github: &dyn Github) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION");
    match github.release() {
        Ok(release) => evaluate(current, &release),
        // Not an error. The sentence says what happened and nothing else, and
        // `behind` stays false so the interface stays quiet.
        Err(why) => UpdateInfo {
            current: current.to_string(),
            latest: String::new(),
            behind: false,
            apk_url: None,
            notes: why,
        },
    }
}

/// A cheap, infallible answer for the start-up hook.
///
/// It reuses a recent answer from the settings table, so starting the app ten
/// times in a day does not ask GitHub ten times, and an app that started with no
/// network does not keep paying for it.
pub fn check(ctx: &Ctx<'_>) -> Option<UpdateInfo> {
    startup_with(ctx, &Http)
}

fn startup_with(ctx: &Ctx<'_>, github: &dyn Github) -> Option<UpdateInfo> {
    let current = env!("CARGO_PKG_VERSION");

    if let Some((latest, apk_url)) = cached(ctx) {
        return if newer(&latest, current) {
            Some(UpdateInfo {
                current: current.to_string(),
                latest,
                behind: true,
                apk_url,
                notes: "نسخه تازه در دسترس است".to_string(),
            })
        } else {
            None
        };
    }

    let info = check_with(github);
    remember(ctx, &info);
    if info.behind {
        Some(info)
    } else {
        None
    }
}

/// A recent, successful answer — never a failure, so a phone that was offline
/// tries again next time instead of remembering that it could not reach GitHub.
fn cached(ctx: &Ctx<'_>) -> Option<(String, Option<String>)> {
    let conn = ctx.conn().ok()?;
    let at: i64 = db::get_setting(&conn, KEY_AT, "").parse().ok()?;
    if at <= 0 || db::now_ms() - at > CACHE_FOR_MS {
        return None;
    }
    let latest = db::get_setting(&conn, KEY_LATEST, "");
    if latest.is_empty() {
        return None;
    }
    let apk_url = db::get_setting(&conn, KEY_APK, "");
    Some((latest, if apk_url.is_empty() { None } else { Some(apk_url) }))
}

fn remember(ctx: &Ctx<'_>, info: &UpdateInfo) {
    // A failed check is not worth remembering: the next start should try again.
    if info.latest.is_empty() {
        return;
    }
    let Ok(conn) = ctx.conn() else { return };
    let _ = db::put_setting(&conn, KEY_AT, &db::now_ms().to_string());
    let _ = db::put_setting(&conn, KEY_LATEST, &info.latest);
    let _ = db::put_setting(&conn, KEY_APK, info.apk_url.as_deref().unwrap_or(""));
}

// ------------------------------------------------------------------ the download

/// Downloads the newest APK into the app's files folder and returns where it is.
///
/// The bytes go to `<name>.apk.part` first; only a complete file is renamed to
/// `<name>.apk`. A half-written file that another app could be asked to install
/// is exactly what this avoids.
pub fn download_with(ctx: &Ctx<'_>, github: &dyn Github) -> Res<Downloaded> {
    let release = github.release()?;
    let info = evaluate(env!("CARGO_PKG_VERSION"), &release);
    if !info.behind {
        return Err("نسخه تازه‌ای نیست".to_string());
    }
    let url = info
        .apk_url
        .clone()
        .ok_or_else(|| "فایل نسخه تازه پیدا نشد".to_string())?;

    // A plain ASCII name: it travels through an intent, a file provider and
    // another app's file list, and none of those need surprises.
    let name = format!("daftar-{}.apk", ascii_version(&info.latest));
    std::fs::create_dir_all(&ctx.state.dirs.files)
        .map_err(|_| "پوشه فایل‌ها ساخته نشد".to_string())?;
    let part = ctx.state.dirs.files.join(format!("{name}.part"));
    let whole = ctx.state.dirs.files.join(&name);

    // A leftover from a download that died last time is stale by definition.
    let _ = std::fs::remove_file(&part);

    let size = match github.fetch(&url, &part) {
        Ok(size) => size,
        Err(why) => {
            let _ = std::fs::remove_file(&part);
            return Err(why);
        }
    };

    if size <= 0 {
        let _ = std::fs::remove_file(&part);
        return Err("نسخه تازه دانلود نشد".to_string());
    }

    std::fs::rename(&part, &whole).map_err(|_| "نسخه تازه دانلود نشد".to_string())?;
    let size = std::fs::metadata(&whole)
        .map(|m| m.len() as i64)
        .unwrap_or(size);

    Ok(Downloaded {
        path: whole.to_string_lossy().to_string(),
        size,
    })
}

/// Keeps a version usable as a file name: letters, digits, dots, dashes.
fn ascii_version(version: &str) -> String {
    let cleaned: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .take(24)
        .collect();
    if cleaned.is_empty() {
        "new".to_string()
    } else {
        cleaned
    }
}

// --------------------------------------------------------------------- the door

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    let _ = args;
    Some(match op {
        // Always fresh: he pressed the button himself.
        "update.check" => (|| json_of(check_with(&Http)))(),
        "update.download" => (|| {
            let got = download_with(ctx, &Http)?;
            json_of(got)
        })(),
        _ => return None,
    })
}

fn json_of<T: Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|_| "پاسخ ساخته نشد".to_string())
}

/// A release, shaped the way GitHub sends it. Used by the tests and by nothing
/// else, so the real parsing is exercised against the real field names.
#[cfg(test)]
fn release_json(tag: &str, assets: &[(&str, &str)]) -> Value {
    let assets: Vec<Value> = assets
        .iter()
        .map(|(name, url)| json!({ "name": name, "browser_download_url": url }))
        .collect();
    json!({ "tag_name": tag, "assets": assets })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ---------------------------------------------------------------- versions

    #[test]
    fn a_newer_version_beats_an_older_one_as_a_number_not_as_text() {
        // The whole reason this is not a string comparison.
        assert!(newer("1.0.10", "1.0.9"));
        assert!(!newer("1.0.9", "1.0.10"));
        assert!(newer("0.1.2", "0.1.1"));
        assert!(newer("0.2", "0.1.9"));
        assert!(newer("2", "1.99.99"));
        assert!(!newer("0.1.1", "0.1.1"));
        // Trailing zeros carry no meaning: these are the same version.
        assert!(!newer("1.0", "1.0.0"));
        assert!(!newer("1.0.0", "1.0"));
    }

    #[test]
    fn the_v_prefix_is_stripped_and_garbage_cannot_claim_an_update() {
        assert_eq!(trim_v("v0.1.1"), "0.1.1");
        assert_eq!(trim_v("V0.1.1"), "0.1.1");
        assert_eq!(trim_v(" 0.1.1 "), "0.1.1");
        assert!(!newer("v0.1.1", "0.1.1"));

        // A tag nobody can read is not an update, in either direction.
        assert!(!newer("nightly", "0.1.1"));
        assert!(!newer("", "0.1.1"));
        assert!(!newer("0.1.1", "nightly"));
        // A suffix after the numbers still compares as the numbers.
        assert!(newer("0.1.2-beta", "0.1.1"));
        assert!(!newer("0.1.1-beta", "0.1.1"));
    }

    // ------------------------------------------------------------------ parsing

    #[test]
    fn the_tag_is_read_without_its_v_and_the_update_is_recognised() {
        let release = release_json(
            "v0.1.2",
            &[("Daftar-0.1.2.apk", "https://example.test/Daftar-0.1.2.apk")],
        );
        let info = evaluate("0.1.1", &release);

        assert_eq!(info.current, "0.1.1");
        assert_eq!(info.latest, "0.1.2");
        assert!(info.behind);
        assert_eq!(
            info.apk_url.as_deref(),
            Some("https://example.test/Daftar-0.1.2.apk")
        );
        assert!(info.notes.contains("تازه"), "notes: {}", info.notes);
    }

    #[test]
    fn the_same_version_is_not_an_update() {
        let release = release_json(
            "v0.1.1",
            &[("Daftar-0.1.1.apk", "https://example.test/a.apk")],
        );
        let info = evaluate("0.1.1", &release);
        assert!(!info.behind);
        assert_eq!(info.notes, "برنامه به‌روز است");
    }

    #[test]
    fn the_apk_is_picked_from_the_assets() {
        // A signature and a checksum are not the app.
        let release = release_json(
            "v0.2.0",
            &[
                ("Daftar-0.2.0.apk.sig", "https://example.test/a.sig"),
                ("notes.txt", "https://example.test/notes.txt"),
                ("Daftar-0.2.0-arm64.apk", "https://example.test/arm64.apk"),
                ("Daftar-0.2.0-universal.apk", "https://example.test/universal.apk"),
            ],
        );
        let info = evaluate("0.1.1", &release);
        // The universal build wins when several ABIs are attached.
        assert_eq!(
            info.apk_url.as_deref(),
            Some("https://example.test/universal.apk")
        );

        // With only one APK, that one is picked.
        let single = release_json(
            "v0.2.0",
            &[("Daftar-0.2.0.apk", "https://example.test/one.apk")],
        );
        assert_eq!(
            evaluate("0.1.1", &single).apk_url.as_deref(),
            Some("https://example.test/one.apk")
        );

        // No APK at all is not a crash: the update is known, the file is not.
        let empty = release_json("v0.2.0", &[("notes.txt", "https://example.test/n")]);
        let info = evaluate("0.1.1", &empty);
        assert!(info.behind);
        assert!(info.apk_url.is_none());
    }

    // ------------------------------------------------------------------ no network

    #[test]
    fn no_network_is_not_an_error() {
        let offline = Stub::failing("به اینترنت وصل نشد");
        let info = check_with(&offline);

        assert!(!info.behind, "an unreachable GitHub is never an update");
        assert_eq!(info.latest, "");
        assert!(info.apk_url.is_none());
        assert_eq!(info.notes, "به اینترنت وصل نشد");
        assert!(
            info.notes.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
            "notes must be Persian: {}",
            info.notes
        );

        // A rate limit is the same shape, with its own sentence.
        let limited = Stub::failing("گیت‌هاب فعلاً پاسخ نمی‌دهد");
        let info = check_with(&limited);
        assert!(!info.behind);
        assert!(info.notes.contains("گیت‌هاب"));
    }

    #[test]
    fn a_check_against_a_real_answer_reports_what_it_finds() {
        let github = Stub::answering(release_json(
            "v0.2.0",
            &[("Daftar-0.2.0.apk", "https://example.test/a.apk")],
        ));
        let info = check_with(&github);
        assert!(info.behind);
        assert_eq!(info.latest, "0.2.0");
    }

    // ------------------------------------------------------------------ download

    #[test]
    fn a_download_lands_as_a_plain_ascii_apk_and_leaves_no_part_file() {
        let bench = Bench::new("update-ok");
        let bytes = b"PK\x03\x04 not really an apk".to_vec();
        let github = Stub::answering(release_json(
            "v0.2.0",
            &[("Daftar-0.2.0.apk", "https://example.test/a.apk")],
        ))
        .with_bytes(bytes.clone());

        let got = download_with(&bench.ctx(), &github).unwrap();

        let path = std::path::Path::new(&got.path);
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "daftar-0.2.0.apk"
        );
        assert!(path.is_absolute(), "a path for an intent must be absolute");
        assert_eq!(got.size, bytes.len() as i64);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(
            !path.with_extension("apk.part").exists(),
            "the .part file is gone"
        );
        assert_eq!(bench.files().len(), 1, "one file, and it is the APK");
    }

    #[test]
    fn a_failed_download_leaves_nothing_installable() {
        let bench = Bench::new("update-fail");
        // It writes half the file and then the connection dies.
        let github = Stub::answering(release_json(
            "v0.2.0",
            &[("Daftar-0.2.0.apk", "https://example.test/a.apk")],
        ))
        .breaking_after(7);

        let why = download_with(&bench.ctx(), &github).unwrap_err();
        assert_eq!(why, "نسخه تازه دانلود نشد");
        assert!(
            bench.files().is_empty(),
            "no .apk and no .part may be left: {:?}",
            bench.files()
        );
    }

    #[test]
    fn there_is_nothing_to_download_when_the_app_is_current() {
        let bench = Bench::new("update-current");
        let github = Stub::answering(release_json(
            "v0.0.1",
            &[("Daftar-0.0.1.apk", "https://example.test/old.apk")],
        ))
        .with_bytes(b"old".to_vec());

        let why = download_with(&bench.ctx(), &github).unwrap_err();
        assert_eq!(why, "نسخه تازه‌ای نیست");
        assert!(bench.files().is_empty());
    }

    // ------------------------------------------------------------------- the cache

    #[test]
    fn the_startup_check_is_infallible_and_can_answer_from_the_cache() {
        let bench = Bench::new("update-cache");
        let current = env!("CARGO_PKG_VERSION");

        // Offline, with nothing remembered: no answer, no panic, and nothing
        // gets written down — the next start should try again.
        let offline = Stub::failing("به اینترنت وصل نشد");
        assert!(startup_with(&bench.ctx(), &offline).is_none());
        assert!(cached(&bench.ctx()).is_none(), "a failure is never cached");

        // One good look at a much newer release.
        let online = Stub::answering(release_json(
            "v999.0.0",
            &[("Daftar-999.0.0.apk", "https://example.test/x.apk")],
        ));
        let first = startup_with(&bench.ctx(), &online).expect("999.0.0 is newer");
        assert_eq!(first.latest, "999.0.0");
        assert_eq!(first.current, current);

        // The network is gone again, but the answer is remembered.
        let offline_again = Stub::failing("به اینترنت وصل نشد");
        let second = startup_with(&bench.ctx(), &offline_again)
            .expect("the remembered answer still stands");
        assert_eq!(second.latest, "999.0.0");
        assert!(second.behind);
        assert_eq!(
            second.apk_url.as_deref(),
            Some("https://example.test/x.apk")
        );
        assert!(
            offline_again.asked.borrow().is_empty(),
            "a cached answer costs no request"
        );
    }

    // -------------------------------------------------------------------- the door

    #[test]
    fn handle_answers_only_its_own_names() {
        let bench = Bench::new("update-door");
        let ctx = bench.ctx();
        assert!(handle(&ctx, "update.nope", &json!({})).is_none());
        assert!(handle(&ctx, "share.invoiceText", &json!({})).is_none());
    }

    // ---------------------------------------------------------------------- stubs

    /// A GitHub that answers from a script instead of the network.
    struct Stub {
        answer: Res<Value>,
        bytes: Option<Vec<u8>>,
        /// Writes this many bytes and then fails, like a dropped connection.
        break_after: Option<usize>,
        asked: RefCell<Vec<String>>,
    }

    impl Stub {
        fn answering(release: Value) -> Self {
            Self {
                answer: Ok(release),
                bytes: None,
                break_after: None,
                asked: RefCell::new(Vec::new()),
            }
        }

        fn failing(why: &str) -> Self {
            Self {
                answer: Err(why.to_string()),
                bytes: None,
                break_after: None,
                asked: RefCell::new(Vec::new()),
            }
        }

        fn with_bytes(mut self, bytes: Vec<u8>) -> Self {
            self.bytes = Some(bytes);
            self
        }

        fn breaking_after(mut self, bytes: usize) -> Self {
            self.break_after = Some(bytes);
            self
        }
    }

    impl Github for Stub {
        fn release(&self) -> Res<Value> {
            self.asked.borrow_mut().push("release".to_string());
            self.answer.clone()
        }

        fn fetch(&self, url: &str, to: &Path) -> Res<i64> {
            self.asked.borrow_mut().push(url.to_string());
            if let Some(limit) = self.break_after {
                // Half a file on disk, then the connection dies.
                let partial = self.bytes.clone().unwrap_or_default();
                let kept = partial.len().min(limit);
                let _ = std::fs::write(to, &partial[..kept]);
                return Err("نسخه تازه دانلود نشد".to_string());
            }
            let bytes = self.bytes.clone().unwrap_or_default();
            std::fs::write(to, &bytes).map_err(|_| "نسخه تازه دانلود نشد".to_string())?;
            Ok(bytes.len() as i64)
        }
    }

    /// A context with a real files folder to download into.
    struct Bench {
        dir: std::path::PathBuf,
        state: Option<crate::AppState>,
    }

    impl Bench {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("daftar-{tag}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp dir");
            std::fs::create_dir_all(dir.join("files")).expect("files dir");
            let state = crate::AppState {
                db: std::sync::Mutex::new(db::memory()),
                dirs: crate::Dirs {
                    data: dir.clone(),
                    files: dir.join("files"),
                },
            };
            Self {
                dir,
                state: Some(state),
            }
        }

        fn ctx(&self) -> Ctx<'_> {
            Ctx {
                state: self.state.as_ref().expect("state"),
            }
        }

        /// What is actually in the app's files folder.
        fn files(&self) -> Vec<String> {
            std::fs::read_dir(&self.dir.join("files"))
                .expect("files dir")
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                .collect()
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            self.state = None;
            for _ in 0..10 {
                if std::fs::remove_dir_all(&self.dir).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
}
