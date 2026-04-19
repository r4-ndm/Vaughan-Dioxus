//! Manifest-driven installs for native dApp helpers (e.g. local PulseX `pulsex-server`).
//!
//! Flow: fetch manifest → pick artifact for `current_target_slug()` → download → SHA-256 verify →
//! extract `pulsex-server` under the app data directory → persist path in [`crate::core::persistence::UserPreferences`].

use crate::core::persistence::{NativeDappInstallRecord, PersistenceHandle};
use crate::error::WalletError;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task;

pub const PULSEX_NATIVE_ID: &str = "pulsex-local";

/// Public manifest on the default branch (Option B: update checks use this URL).
pub const PULSEX_MANIFEST_URL_DEFAULT: &str =
    "https://raw.githubusercontent.com/r4-ndm/Vaughan-Dioxus/main/PulseX/pulsex-manifest.json";

const EMBEDDED_MANIFEST: &str = include_str!("../../PulseX/pulsex-manifest.json");

/// Bundled copy of the Linux amd64 archive (same SHA-256 as `pulsex-manifest.json`).
/// Used when the manifest URL is 404 or unreachable so Install still works offline / before you publish artifacts.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const EMBEDDED_PULSEX_TAR_GZ: &[u8] = include_bytes!("../../PulseX/pulsex-server_1.1.4_linux_amd64.tar.gz");

#[derive(Debug, Deserialize)]
pub struct PulsexManifest {
    pub schema_version: u32,
    pub id: String,
    #[serde(default)]
    pub manifest_url: Option<String>,
    pub releases: Vec<PulsexRelease>,
}

#[derive(Debug, Deserialize)]
pub struct PulsexRelease {
    pub version: String,
    pub artifacts: Vec<PulsexArtifact>,
}

#[derive(Debug, Deserialize)]
pub struct PulsexArtifact {
    pub target: String,
    pub url: String,
    pub archive_sha256: String,
}

#[derive(Debug, Clone)]
pub struct PulsexInstallOutcome {
    pub version: String,
    pub binary_path: PathBuf,
}

pub fn current_target_slug() -> Option<&'static str> {
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some("linux_x86_64")
    } else {
        None
    }
}

pub fn embedded_manifest_str() -> &'static str {
    EMBEDDED_MANIFEST
}

pub fn parse_manifest_json(s: &str) -> Result<PulsexManifest, WalletError> {
    serde_json::from_str(s).map_err(|e| WalletError::InvalidData(format!("manifest JSON: {e}")))
}

pub async fn fetch_manifest_from_url(url: &str) -> Result<PulsexManifest, WalletError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| WalletError::NetworkError(e.to_string()))?;

    let text = client
        .get(url)
        .send()
        .await
        .map_err(|e| WalletError::NetworkError(e.to_string()))?
        .error_for_status()
        .map_err(|e| WalletError::NetworkError(e.to_string()))?
        .text()
        .await
        .map_err(|e| WalletError::NetworkError(e.to_string()))?;

    parse_manifest_json(&text)
}

/// Load manifest: try network first when `prefer_remote`, else embedded copy.
pub async fn load_pulsex_manifest(prefer_remote: bool) -> Result<PulsexManifest, WalletError> {
    if prefer_remote {
        match fetch_manifest_from_url(PULSEX_MANIFEST_URL_DEFAULT).await {
            Ok(m) => return Ok(m),
            Err(e) => {
                tracing::warn!(
                    target: "vaughan_core",
                    "pulsex manifest fetch failed, using embedded: {}",
                    e
                );
            }
        }
    }
    parse_manifest_json(EMBEDDED_MANIFEST)
}

fn native_dapps_data_dir() -> Result<PathBuf, WalletError> {
    let base = dirs::data_dir().ok_or_else(|| WalletError::StorageError("no data_dir".into()))?;
    Ok(base.join("vaughan").join("native_dapps"))
}

fn select_artifact<'a>(
    manifest: &'a PulsexManifest,
    target: &str,
) -> Option<(&'a str, &'a PulsexArtifact)> {
    for rel in &manifest.releases {
        for art in &rel.artifacts {
            if art.target == target {
                return Some((rel.version.as_str(), art));
            }
        }
    }
    None
}

fn verify_sha256(data: &[u8], expected_hex_lower: &str) -> Result<(), WalletError> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let out = hasher.finalize();
    let hex = hex::encode(out);
    let exp = expected_hex_lower.to_lowercase();
    if hex != exp {
        return Err(WalletError::Other(format!(
            "Checksum mismatch (expected {exp}, got {hex})"
        )));
    }
    Ok(())
}

fn install_extract_sync(dest_dir: &Path, archive_bytes: &[u8]) -> Result<(), WalletError> {
    std::fs::create_dir_all(dest_dir).map_err(|e| WalletError::StorageError(e.to_string()))?;

    let cursor = std::io::Cursor::new(archive_bytes);
    let dec = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(dec);

    let mut found = false;
    for entry in archive.entries().map_err(|e| WalletError::Other(e.to_string()))? {
        let mut entry = entry.map_err(|e| WalletError::Other(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| WalletError::Other(e.to_string()))?;
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "pulsex-server")
            .unwrap_or(false)
        {
            let out = dest_dir.join("pulsex-server");
            if out.exists() {
                std::fs::remove_file(&out).map_err(|e| WalletError::StorageError(e.to_string()))?;
            }
            entry
                .unpack(&out)
                .map_err(|e| WalletError::Other(e.to_string()))?;
            found = true;
            break;
        }
    }
    if !found {
        return Err(WalletError::Other(
            "Archive did not contain pulsex-server".into(),
        ));
    }
    Ok(())
}

async fn download_bytes(url: &str) -> Result<Vec<u8>, WalletError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| WalletError::NetworkError(format!("HTTP client: {e}")))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| WalletError::NetworkError(format!("GET {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let hint = if status == reqwest::StatusCode::NOT_FOUND {
            " (file may not be on GitHub yet — build includes a fallback archive when possible)"
        } else {
            ""
        };
        return Err(WalletError::NetworkError(format!(
            "GET {url} returned {status}{hint}"
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| WalletError::NetworkError(format!("reading body from {url}: {e}")))?;
    Ok(bytes.to_vec())
}

fn try_embedded_pulsex_archive(expected_sha256: &str) -> Option<Vec<u8>> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        verify_sha256(EMBEDDED_PULSEX_TAR_GZ, expected_sha256).ok()?;
        Some(EMBEDDED_PULSEX_TAR_GZ.to_vec())
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = expected_sha256;
        None
    }
}

async fn resolve_pulsex_archive_bytes(url: &str, expected_sha256: &str) -> Result<Vec<u8>, WalletError> {
    if let Ok(p) = std::env::var("VAUGHAN_PULSEX_ARCHIVE") {
        let p = p.trim();
        if !p.is_empty() {
            let b = tokio::fs::read(p)
                .await
                .map_err(|e| WalletError::Other(format!("Could not read VAUGHAN_PULSEX_ARCHIVE ({p}): {e}")))?;
            verify_sha256(&b, expected_sha256)?;
            tracing::info!(target: "vaughan_core", "pulsex: using archive from VAUGHAN_PULSEX_ARCHIVE");
            return Ok(b);
        }
    }

    match download_bytes(url).await {
        Ok(b) => Ok(b),
        Err(dl) => {
            if let Some(emb) = try_embedded_pulsex_archive(expected_sha256) {
                tracing::warn!(
                    target: "vaughan_core",
                    "pulsex: download failed ({}); installing from embedded archive",
                    dl
                );
                Ok(emb)
            } else {
                Err(dl)
            }
        }
    }
}

/// Download, verify SHA-256, extract, chmod when applicable, persist preferences.
pub async fn download_install_pulsex_for_current_target(
    manifest: &PulsexManifest,
    persistence: Arc<PersistenceHandle>,
) -> Result<PulsexInstallOutcome, WalletError> {
    let slug = current_target_slug().ok_or_else(|| {
        WalletError::Other(
            "PulseX server install is only available on Linux x86-64.".into(),
        )
    })?;

    let (version, art) = select_artifact(manifest, slug).ok_or_else(|| {
        WalletError::Other(format!("No artifact for target {slug} in manifest."))
    })?;

    let bytes = resolve_pulsex_archive_bytes(&art.url, &art.archive_sha256).await?;
    verify_sha256(&bytes, &art.archive_sha256)?;

    let base = native_dapps_data_dir()?;
    let dest_dir = base.join(PULSEX_NATIVE_ID).join(version);
    let bin_path = dest_dir.join("pulsex-server");

    let dest_dir_clone = dest_dir.clone();
    task::spawn_blocking(move || install_extract_sync(&dest_dir_clone, &bytes))
        .await
        .map_err(|e| WalletError::Other(format!("install join: {e}")))??;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta =
            std::fs::metadata(&bin_path).map_err(|e| WalletError::StorageError(e.to_string()))?;
        let mut perm = meta.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perm)
            .map_err(|e| WalletError::StorageError(e.to_string()))?;
    }

    let ver_owned = version.to_string();
    let record = NativeDappInstallRecord {
        installed_version: ver_owned.clone(),
        binary_path: bin_path.clone(),
        archive_sha256: art.archive_sha256.clone(),
    };

    persistence
        .update_and_save(|st| {
            let mut prefs = st.preferences.clone().unwrap_or_default();
            prefs
                .native_dapps_v1
                .insert(PULSEX_NATIVE_ID.into(), record.clone());
            st.preferences = Some(prefs);
        })
        .await?;

    Ok(PulsexInstallOutcome {
        version: ver_owned,
        binary_path: bin_path,
    })
}

pub fn pulsex_latest_version(manifest: &PulsexManifest) -> Option<&str> {
    manifest.releases.first().map(|r| r.version.as_str())
}

pub fn pulsex_update_available(
    manifest: &PulsexManifest,
    installed: Option<&NativeDappInstallRecord>,
) -> bool {
    let Some(latest) = pulsex_latest_version(manifest) else {
        return false;
    };
    let Some(ins) = installed else {
        return true;
    };
    ins.installed_version != latest
}

pub fn pulsex_record(persistence: &PersistenceHandle) -> Option<NativeDappInstallRecord> {
    persistence
        .snapshot()
        .preferences
        .unwrap_or_default()
        .native_dapps_v1
        .get(PULSEX_NATIVE_ID)
        .cloned()
}
