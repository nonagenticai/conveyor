#![forbid(unsafe_code)]
//! `conveyor auth token` — the session's machine channel (#760).
//!
//! A Claude session calls the conveyor as a **machine principal**: an opaque licence bearer that
//! `crates/auth` introspects at the licences service (`licence:<subject_id>`), resolved to an
//! `app_user` row of kind `service`. The principal is provisions with `conveyor admin
//! machine-principal ensure`; this module mints and caches the bearer.
//!
//! The mint targets the tenant's **active licence for the conveyor app** and uses the short-lived
//! `POST /licenses/{id}/tokens` route (one day by default), never a long-lived operator token. The
//! raw token is written 0600 and never printed; only the file path and the expiry are.
//!
//! The credential is the dedicated `<session-client>` service account when both env vars are set;
//! otherwise the deployment's existing service credential is used, which is the local shape and
//! works only where that client already holds the licences mint role. The dedicated client holds
//! `license-manager` (mint) and `license-reader` (resolve the tenant's active licence id) — the
//! licences read set does not include `license-manager`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use conveyor_api::Config;
use conveyor_auth::{ClientCredentials, LicenceSyncClient};
use conveyor_store as store;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// The default lifetime of a minted session bearer.
pub const DEFAULT_EXPIRES_DAYS: u32 = 1;
/// The licences route's own ceiling.
pub const MAX_EXPIRES_DAYS: u32 = 365;
/// A cached token is reused while it has more than this left.
pub const REUSE_WINDOW_SECS: i64 = 3600;
/// The session service account's client id.
pub const SESSION_CLIENT_ID_ENV: &str = "CONVEYOR_SESSION_CLIENT_ID";
/// The session service account's client secret.
pub const SESSION_CLIENT_SECRET_ENV: &str = "CONVEYOR_SESSION_CLIENT_SECRET";

/// What the cache file holds. The token is the only secret; the expiry lets a later run decide
/// whether to re-mint.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CachedToken {
    /// The raw opaque bearer. Never printed.
    pub token: String,
    /// RFC 3339, as the licences service stated it.
    pub expires_at: String,
}

/// True when `expires_at` is more than [`REUSE_WINDOW_SECS`] away from `now`.
///
/// An unparseable expiry is **not** fresh: a cache file whose timestamp cannot be read is
/// re-minted, never trusted.
pub fn is_fresh(expires_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(expires_at)
        .map(|at| at.with_timezone(&Utc) - now > Duration::seconds(REUSE_WINDOW_SECS))
        .unwrap_or(false)
}

/// `~/.conveyor/<tenant>.token`.
pub fn cache_path(tenant_slug: &str) -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_owned())?;
    Ok(PathBuf::from(home)
        .join(".conveyor")
        .join(format!("{tenant_slug}.token")))
}

/// Read a cache file. A missing or malformed file is `None`, never an error: the caller mints.
pub fn read_cache(path: &Path) -> Option<CachedToken> {
    let raw = fs::read_to_string(path).ok()?;
    let cached: CachedToken = serde_json::from_str(raw.trim()).ok()?;
    (!cached.token.trim().is_empty()).then_some(cached)
}

/// Write the cache file and its directory, both owner-only.
pub fn write_cache(path: &Path, token: &str, expires_at: &str) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        // Only tighten a directory we create: an existing parent (a caller's `--out /tmp/x`)
        // belongs to the caller, and chmod on it can fail outright.
        let created = !dir.exists();
        fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        if created {
            set_mode(dir, 0o700)?;
        }
    }
    let body = serde_json::to_vec(&CachedToken {
        token: token.to_owned(),
        expires_at: expires_at.to_owned(),
    })
    .map_err(|error| format!("encode the token cache: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    file.write_all(&body)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", path.display()))?;
    set_mode(path, 0o600)
}

fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| format!("chmod {}: {error}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

/// A token file holds either the cache JSON or a bare token (one line, as an operator copies it).
pub fn token_from_file(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('{')
        && let Ok(cached) = serde_json::from_str::<CachedToken>(trimmed)
    {
        return cached.token;
    }
    trimmed.to_owned()
}

/// The credential to mint with: the dedicated session client when both env vars are set, else the
/// deployment's service credential.
pub fn session_credentials(config: &Config) -> Result<ClientCredentials, String> {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    match (read(SESSION_CLIENT_ID_ENV), read(SESSION_CLIENT_SECRET_ENV)) {
        (Some(id), Some(secret)) => {
            let url = config
                .service_token_url
                .as_deref()
                .filter(|url| !url.trim().is_empty())
                .ok_or_else(|| "SERVICE_TOKEN_URL is not set".to_owned())?;
            ClientCredentials::new(url, &id, &secret).map_err(|error| error.to_string())
        }
        (None, None) => config
            .service_credentials()
            .map_err(|error| error.to_string()),
        _ => Err(format!(
            "set both {SESSION_CLIENT_ID_ENV} and {SESSION_CLIENT_SECRET_ENV}, or neither"
        )),
    }
}

/// Mint a bearer on the tenant's active conveyor licence.
pub async fn mint(
    config: &Config,
    pool: &PgPool,
    tenant_slug: &str,
    expires_days: u32,
) -> Result<CachedToken, String> {
    if expires_days == 0 || expires_days > MAX_EXPIRES_DAYS {
        return Err(format!(
            "--expires-days must be between 1 and {MAX_EXPIRES_DAYS}"
        ));
    }
    let base = config
        .licences_base_url
        .as_deref()
        .filter(|base| !base.trim().is_empty())
        .ok_or_else(|| "LICENCES_BASE_URL is not set".to_owned())?;
    let tenant = store::tenants::id_by_slug(pool, tenant_slug)
        .await
        .map_err(|error| format!("read tenant {tenant_slug:?}: {error}"))?
        .ok_or_else(|| format!("no tenant {tenant_slug:?}"))?;
    let client = LicenceSyncClient::with_credentials(base, session_credentials(config)?)
        .map_err(|error| format!("licences client: {error}"))?;
    let license_id = client
        .active_license_id(tenant, &config.app_key)
        .await
        .map_err(|error| format!("licences active read: {error}"))?
        .ok_or_else(|| {
            format!(
                "tenant {tenant_slug:?} has no active {:?} licence",
                config.app_key
            )
        })?;
    let minted = client
        .create_license_token(tenant, &license_id, expires_days)
        .await
        .map_err(|error| format!("licences mint: {error}"))?;
    Ok(CachedToken {
        token: minted.token,
        expires_at: minted.expires_at,
    })
}

/// The command: reuse a fresh cache, else mint and write it. Returns the file it wrote/read.
pub async fn token_command(
    config: &Config,
    pool: &PgPool,
    tenant_slug: &str,
    expires_days: u32,
    out: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, String> {
    let path = match out {
        Some(path) => path,
        None => cache_path(tenant_slug)?,
    };
    if !force
        && let Some(cached) = read_cache(&path)
        && is_fresh(&cached.expires_at, Utc::now())
    {
        println!(
            "{}",
            serde_json::json!({
                "token_file": path.display().to_string(),
                "expires_at": cached.expires_at,
                "reused": true,
            })
        );
        return Ok(path);
    }
    let minted = mint(config, pool, tenant_slug, expires_days).await?;
    write_cache(&path, &minted.token, &minted.expires_at)?;
    println!(
        "{}",
        serde_json::json!({
            "token_file": path.display().to_string(),
            "expires_at": minted.expires_at,
            "reused": false,
        })
    );
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn freshness_is_the_one_hour_window_and_unparseable_is_not_fresh() {
        assert!(is_fresh("2026-11-26T13:00:01Z", now()));
        assert!(!is_fresh("2026-11-26T13:00:00Z", now()));
        assert!(!is_fresh("2026-11-26T12:30:00Z", now()));
        assert!(!is_fresh("not-a-date", now()));
    }

    #[test]
    fn a_bare_token_and_the_cache_json_both_parse() {
        assert_eq!(token_from_file("  abc123\n"), "abc123");
        let json = serde_json::json!({"token": "abc123", "expires_at": "2026-11-27T00:00:00Z"});
        assert_eq!(token_from_file(&json.to_string()), "abc123");
    }

    #[test]
    fn writing_into_an_existing_directory_succeeds_and_the_file_is_owner_only() {
        // The parent is the OS temp dir: it must not be chmod'ed, and the write must still work.
        let path =
            std::env::temp_dir().join(format!("conveyor-session-file-{}", uuid::Uuid::now_v7()));
        write_cache(&path, "secret-token", "2026-11-27T00:00:00Z").unwrap();
        assert_eq!(read_cache(&path).expect("round trip").token, "secret-token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        fs::remove_file(&path).ok();
    }

    #[test]
    fn the_cache_round_trips_and_is_owner_only() {
        let dir = std::env::temp_dir().join(format!("conveyor-session-{}", uuid::Uuid::now_v7()));
        let path = dir.join("nonagentic.token");
        write_cache(&path, "secret-token", "2026-11-27T00:00:00Z").unwrap();
        let cached = read_cache(&path).expect("a written cache reads back");
        assert_eq!(cached.token, "secret-token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        fs::remove_dir_all(&dir).ok();
    }
}
