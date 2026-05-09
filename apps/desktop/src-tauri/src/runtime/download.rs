use crate::runtime::manifest::{BootstrapReleaseManifest, RuntimeManifest};
use crate::runtime::paths::{
    ensure_executable_bit, managed_pack_dir, runtime_root, stage_marker_path,
};
use crate::runtime::status::{RuntimeOperation, RuntimeOperationKind, RuntimeOperationStage};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapDownloadOutcome {
    pub archive_path: PathBuf,
    pub staging_path: PathBuf,
    pub managed_root: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapResumeMetadata {
    pub manifest_url: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub downloaded_bytes: u64,
    pub archive_size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlanPaths {
    pub archive_path: PathBuf,
    pub staging_path: PathBuf,
    pub resume_metadata_path: PathBuf,
}

pub fn bootstrap_download_dir(app_data_dir: &Path, pack_version: &str) -> PathBuf {
    runtime_root(app_data_dir)
        .join(".downloads")
        .join(pack_version)
}

pub fn bootstrap_download_plan_paths(
    app_data_dir: &Path,
    pack_version: &str,
    archive_name: &str,
) -> DownloadPlanPaths {
    let download_dir = bootstrap_download_dir(app_data_dir, pack_version);
    DownloadPlanPaths {
        archive_path: download_dir.join(archive_name),
        staging_path: runtime_root(app_data_dir).join(format!(".{pack_version}.staging")),
        resume_metadata_path: download_dir.join("resume.json"),
    }
}

pub fn download_and_activate_remote_runtime<F>(
    source_manifest_url: &str,
    release: &BootstrapReleaseManifest,
    app_data_dir: &Path,
    public_key_base64: &str,
    mut on_progress: F,
) -> Result<BootstrapDownloadOutcome, String>
where
    F: FnMut(RuntimeOperation),
{
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("Failed to create bootstrap HTTP client: {error}"))?;

    download_and_activate_remote_runtime_with_fetch(
        source_manifest_url,
        release,
        app_data_dir,
        public_key_base64,
        |url| {
            let response = client
                .get(url)
                .send()
                .map_err(|error| format!("Failed to fetch remote runtime archive: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!(
                    "Remote runtime archive request failed with HTTP status {status}"
                ));
            }
            Ok(Box::new(response) as Box<dyn Read>)
        },
        &mut on_progress,
    )
}

pub fn download_and_activate_remote_runtime_with_fetch<F, R>(
    source_manifest_url: &str,
    release: &BootstrapReleaseManifest,
    app_data_dir: &Path,
    public_key_base64: &str,
    fetch_archive: F,
    on_progress: &mut (impl FnMut(RuntimeOperation) + ?Sized),
) -> Result<BootstrapDownloadOutcome, String>
where
    F: FnOnce(&str) -> Result<R, String>,
    R: Read,
{
    release.verify_signature(public_key_base64)?;

    let paths = bootstrap_download_plan_paths(
        app_data_dir,
        &release.pack_version,
        archive_file_name(release),
    );
    if let Some(parent) = paths.archive_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create download dir {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::create_dir_all(runtime_root(app_data_dir)).map_err(|error| {
        format!(
            "Failed to create runtime root {}: {error}",
            runtime_root(app_data_dir).display()
        )
    })?;

    emit_progress(
        on_progress,
        RuntimeOperationStage::Downloading,
        "Descargando runtime remoto confiable",
        Some(0),
        Some(0),
        Some(release.archive_size),
        true,
    );

    let mut archive_reader = fetch_archive(&release.archive_url)?;
    let tmp_archive_path = paths.archive_path.with_extension("download.tmp");
    let mut archive_file = fs::File::create(&tmp_archive_path).map_err(|error| {
        format!(
            "Failed to create temporary archive {}: {error}",
            tmp_archive_path.display()
        )
    })?;
    let mut downloaded_bytes = 0u64;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; DEFAULT_DOWNLOAD_CHUNK_SIZE];
    let mut last_reported_progress = 0u8;

    loop {
        let read = archive_reader
            .read(&mut buffer)
            .map_err(|error| format!("Failed while reading remote runtime archive: {error}"))?;
        if read == 0 {
            break;
        }
        archive_file
            .write_all(&buffer[..read])
            .map_err(|error| format!("Failed while writing runtime archive: {error}"))?;
        hasher.update(&buffer[..read]);
        downloaded_bytes += read as u64;

        let progress = progress_percent(downloaded_bytes, release.archive_size).unwrap_or(99);
        if progress >= last_reported_progress.saturating_add(5)
            || downloaded_bytes == release.archive_size
        {
            last_reported_progress = progress;
            emit_progress(
                on_progress,
                RuntimeOperationStage::Downloading,
                "Descargando runtime remoto confiable",
                Some(progress.min(99)),
                Some(downloaded_bytes),
                Some(release.archive_size),
                true,
            );
        }
    }

    fs::rename(&tmp_archive_path, &paths.archive_path).map_err(|error| {
        format!(
            "Failed to finalize downloaded archive from {} to {}: {error}",
            tmp_archive_path.display(),
            paths.archive_path.display()
        )
    })?;

    write_resume_metadata(
        &paths.resume_metadata_path,
        &BootstrapResumeMetadata {
            manifest_url: source_manifest_url.to_string(),
            archive_url: release.archive_url.clone(),
            archive_sha256: release.archive_sha256.clone(),
            downloaded_bytes,
            archive_size: release.archive_size,
        },
    )?;

    if downloaded_bytes != release.archive_size {
        return Err(format!(
            "Downloaded archive size mismatch: expected {} bytes, got {}",
            release.archive_size, downloaded_bytes
        ));
    }

    let archive_sha256 = format!("{:x}", hasher.finalize());
    if archive_sha256 != release.archive_sha256 {
        return Err(format!(
            "Downloaded runtime archive checksum mismatch: expected {}, got {}",
            release.archive_sha256, archive_sha256
        ));
    }

    emit_progress(
        on_progress,
        RuntimeOperationStage::Verifying,
        "Verificando integridad del runtime remoto descargado",
        Some(100),
        Some(downloaded_bytes),
        Some(release.archive_size),
        false,
    );

    let staging_path = paths.staging_path.clone();
    if staging_path.exists() {
        fs::remove_dir_all(&staging_path).map_err(|error| {
            format!(
                "Failed to clean staging dir {}: {error}",
                staging_path.display()
            )
        })?;
    }
    fs::create_dir_all(&staging_path).map_err(|error| {
        format!(
            "Failed to create staging dir {}: {error}",
            staging_path.display()
        )
    })?;
    write_stage_marker(app_data_dir, &release.pack_version, "extracting")?;

    emit_progress(
        on_progress,
        RuntimeOperationStage::Hydrating,
        "Extrayendo runtime remoto a staging",
        Some(100),
        Some(downloaded_bytes),
        Some(release.archive_size),
        false,
    );
    extract_archive_to_staging(&paths.archive_path, &staging_path)?;

    let manifest_path = staging_path.join("manifest.json");
    let runtime_manifest = RuntimeManifest::load_from_path(&manifest_path)?;
    verify_extracted_runtime(&staging_path, &runtime_manifest, &release.pack_version)?;

    emit_progress(
        on_progress,
        RuntimeOperationStage::Activating,
        "Activando runtime remoto verificado",
        Some(100),
        Some(downloaded_bytes),
        Some(release.archive_size),
        false,
    );
    let managed_root = managed_pack_dir(app_data_dir, &release.pack_version);
    if managed_root.exists() {
        fs::remove_dir_all(&managed_root).map_err(|error| {
            format!(
                "Failed to remove existing managed runtime {}: {error}",
                managed_root.display()
            )
        })?;
    }
    fs::rename(&staging_path, &managed_root).map_err(|error| {
        format!(
            "Failed to promote runtime from {} to {}: {error}",
            staging_path.display(),
            managed_root.display()
        )
    })?;

    let stage_marker = stage_marker_path(app_data_dir, &release.pack_version);
    if stage_marker.exists() {
        fs::remove_file(&stage_marker).map_err(|error| {
            format!(
                "Failed to remove stage marker {}: {error}",
                stage_marker.display()
            )
        })?;
    }

    Ok(BootstrapDownloadOutcome {
        archive_path: paths.archive_path,
        staging_path: paths.staging_path,
        managed_root,
    })
}

fn archive_file_name(release: &BootstrapReleaseManifest) -> &str {
    release
        .archive_url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("runtime-pack.archive")
}

fn extract_archive_to_staging(archive_path: &Path, staging_path: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path).map_err(|error| {
        format!(
            "Failed to open runtime archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|error| {
        format!(
            "Failed to read runtime archive {}: {error}",
            archive_path.display()
        )
    })?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read archive entry #{index}: {error}"))?;
        let Some(safe_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err(format!("Archive entry {} has an unsafe path", entry.name()));
        };
        let output_path = staging_path.join(safe_name);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output_path).map_err(|error| {
                format!(
                    "Failed to create extracted directory {}: {error}",
                    output_path.display()
                )
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create extracted parent {}: {error}",
                    parent.display()
                )
            })?;
        }
        let mut output_file = fs::File::create(&output_path).map_err(|error| {
            format!(
                "Failed to create extracted file {}: {error}",
                output_path.display()
            )
        })?;
        std::io::copy(&mut entry, &mut output_file).map_err(|error| {
            format!("Failed to extract archive entry {}: {error}", entry.name())
        })?;
    }

    Ok(())
}

fn verify_extracted_runtime(
    staging_path: &Path,
    manifest: &RuntimeManifest,
    expected_pack_version: &str,
) -> Result<(), String> {
    if manifest.pack_version != expected_pack_version {
        return Err(format!(
            "Extracted runtime pack version mismatch: expected {}, got {}",
            expected_pack_version, manifest.pack_version
        ));
    }

    for entry in manifest.all_entries() {
        let target = staging_path.join(&entry.path);
        if !target.exists() {
            return Err(format!("Extracted runtime entry missing: {}", entry.path));
        }
        let bytes = fs::read(&target).map_err(|error| {
            format!(
                "Failed to read extracted entry {}: {error}",
                target.display()
            )
        })?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if sha256 != entry.sha256 {
            return Err(format!(
                "Extracted runtime checksum mismatch for {}",
                entry.path
            ));
        }
        ensure_executable_bit(&target, entry.executable)?;
    }

    Ok(())
}

fn write_resume_metadata(path: &Path, metadata: &BootstrapResumeMetadata) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create resume metadata dir {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(metadata)
            .map_err(|error| format!("Failed to serialize bootstrap resume metadata: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "Failed to write resume metadata {}: {error}",
            path.display()
        )
    })
}

fn write_stage_marker(app_data_dir: &Path, pack_version: &str, stage: &str) -> Result<(), String> {
    let marker_path = stage_marker_path(app_data_dir, pack_version);
    fs::write(
        &marker_path,
        serde_json::to_vec_pretty(
            &serde_json::json!({ "pack_version": pack_version, "stage": stage }),
        )
        .map_err(|error| format!("Failed to serialize stage marker: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "Failed to write stage marker {}: {error}",
            marker_path.display()
        )
    })
}

fn progress_percent(downloaded_bytes: u64, total_bytes: u64) -> Option<u8> {
    (total_bytes > 0).then(|| ((downloaded_bytes.saturating_mul(100)) / total_bytes).min(100) as u8)
}

fn emit_progress(
    on_progress: &mut (impl FnMut(RuntimeOperation) + ?Sized),
    stage: RuntimeOperationStage,
    summary: &str,
    progress_percent: Option<u8>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    retryable: bool,
) {
    on_progress(RuntimeOperation {
        kind: RuntimeOperationKind::Bootstrap,
        stage,
        summary: summary.to_string(),
        progress_percent,
        downloaded_bytes,
        total_bytes,
        retryable,
    });
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    pub(crate) fn sample_release(
        signature: String,
        archive_sha256: String,
        archive_size: u64,
    ) -> BootstrapReleaseManifest {
        BootstrapReleaseManifest {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: crate::runtime::paths::current_runtime_platform(),
            pack_version: "2026.05.1".to_string(),
            archive_url: "https://example.com/runtime-pack.zip".to_string(),
            archive_sha256,
            archive_size,
            signature,
        }
    }

    pub(crate) fn build_signed_release(archive_bytes: &[u8]) -> (BootstrapReleaseManifest, String) {
        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let unsigned = sample_release(
            String::new(),
            format!("{:x}", Sha256::digest(archive_bytes)),
            archive_bytes.len() as u64,
        );
        let signature = signing_key.sign(unsigned.signature_payload().as_bytes());
        let release = BootstrapReleaseManifest {
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            ..unsigned
        };
        let public_key = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().to_bytes());
        (release, public_key)
    }

    pub(crate) fn runtime_archive_bytes() -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = SimpleFileOptions::default();
            zip.add_directory("python/", options).expect("python dir");
            zip.add_directory("python/bin/", options)
                .expect("python bin dir");
            zip.add_directory("uv/", options).expect("uv dir");
            zip.add_directory("uv/bin/", options).expect("uv bin dir");
            zip.start_file("python/bin/python3", options)
                .expect("python entry");
            zip.write_all(b"python").expect("python bytes");
            zip.start_file("uv/bin/uv", options).expect("uv entry");
            zip.write_all(b"uv").expect("uv bytes");
            let manifest = RuntimeManifest {
                pack_version: "2026.05.1".to_string(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                platform: crate::runtime::paths::current_runtime_platform(),
                payload_profile: "release".to_string(),
                release_injection_required: false,
                external_artifacts_required: vec![],
                python_relpath: "python/bin/python3".to_string(),
                uv_relpath: "uv/bin/uv".to_string(),
                python_files: vec![crate::runtime::manifest::ManifestEntry {
                    path: "python/bin/python3".to_string(),
                    sha256: format!("{:x}", Sha256::digest(b"python")),
                    size: 6,
                    executable: !cfg!(windows),
                }],
                uv_files: vec![crate::runtime::manifest::ManifestEntry {
                    path: "uv/bin/uv".to_string(),
                    sha256: format!("{:x}", Sha256::digest(b"uv")),
                    size: 2,
                    executable: true,
                }],
                script_files: vec![],
                wheelhouse: vec![],
                caches: vec![],
                native_assets: vec![],
            };
            zip.start_file("manifest.json", options)
                .expect("manifest entry");
            zip.write_all(&serde_json::to_vec_pretty(&manifest).expect("manifest json"))
                .expect("manifest bytes");
            zip.finish().expect("finish zip");
        }
        buffer.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::download::test_support::{build_signed_release, runtime_archive_bytes};
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;

    #[test]
    fn computes_bootstrap_download_paths_under_runtime_downloads_dir() {
        let app_data_dir = PathBuf::from("/tmp/entropia-data");

        let paths =
            bootstrap_download_plan_paths(&app_data_dir, "2026.05.1", "runtime-pack.archive");

        assert_eq!(
            paths.archive_path,
            app_data_dir
                .join("runtime")
                .join(".downloads")
                .join("2026.05.1")
                .join("runtime-pack.archive")
        );
        assert_eq!(
            paths.resume_metadata_path,
            app_data_dir
                .join("runtime")
                .join(".downloads")
                .join("2026.05.1")
                .join("resume.json")
        );
    }

    #[test]
    fn keeps_staging_path_sibling_to_managed_runtime_dirs() {
        let app_data_dir = PathBuf::from("/tmp/entropia-data");

        let paths = bootstrap_download_plan_paths(&app_data_dir, "2026.05.1", "runtime-pack.zip");

        assert_eq!(
            paths.staging_path,
            app_data_dir.join("runtime").join(".2026.05.1.staging")
        );
    }

    #[test]
    fn downloads_verifies_extracts_and_activates_remote_runtime_archive() {
        let app_data_dir = tempfile::tempdir().expect("app data dir");
        let archive_bytes = runtime_archive_bytes();
        let (release, public_key) = build_signed_release(&archive_bytes);
        let mut progress = Vec::new();

        let outcome = download_and_activate_remote_runtime_with_fetch(
            "https://example.com/runtime/bootstrap.json",
            &release,
            app_data_dir.path(),
            &public_key,
            |_| Ok(Cursor::new(archive_bytes.clone())),
            &mut |operation| progress.push(operation),
        )
        .expect("remote runtime bootstrap should succeed");

        assert!(outcome.archive_path.is_file());
        assert!(outcome.managed_root.join("manifest.json").is_file());
        assert!(outcome.managed_root.join("python/bin/python3").is_file());
        assert!(progress
            .iter()
            .any(|item| item.stage == RuntimeOperationStage::Downloading));
        assert!(progress
            .iter()
            .any(|item| item.stage == RuntimeOperationStage::Hydrating));
        assert!(progress
            .iter()
            .any(|item| item.stage == RuntimeOperationStage::Activating));
    }

    #[test]
    fn rejects_remote_archive_when_checksum_does_not_match() {
        let app_data_dir = tempfile::tempdir().expect("app data dir");
        let archive_bytes = runtime_archive_bytes();
        let (mut release, public_key) = build_signed_release(&archive_bytes);
        release.archive_sha256 = "bad-sha".to_string();
        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let signature = signing_key.sign(release.signature_payload().as_bytes());
        release.signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        let error = download_and_activate_remote_runtime_with_fetch(
            "https://example.com/runtime/bootstrap.json",
            &release,
            app_data_dir.path(),
            &public_key,
            |_| Ok(Cursor::new(archive_bytes.clone())),
            &mut |_| {},
        )
        .expect_err("checksum mismatch must fail");

        assert!(error.contains("checksum mismatch"));
    }

    #[test]
    fn rejects_remote_archive_when_signature_is_invalid() {
        let app_data_dir = tempfile::tempdir().expect("app data dir");
        let archive_bytes = runtime_archive_bytes();
        let (mut release, public_key) = build_signed_release(&archive_bytes);
        release.signature = base64::engine::general_purpose::STANDARD.encode([1u8; 64]);

        let error = download_and_activate_remote_runtime_with_fetch(
            "https://example.com/runtime/bootstrap.json",
            &release,
            app_data_dir.path(),
            &public_key,
            |_| Ok(Cursor::new(archive_bytes.clone())),
            &mut |_| {},
        )
        .expect_err("signature mismatch must fail");

        assert!(error.contains("signature"));
    }

    #[test]
    fn rejects_remote_archive_with_missing_manifest_entry_after_extract() {
        let app_data_dir = tempfile::tempdir().expect("app data dir");
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = SimpleFileOptions::default();
            zip.start_file("manifest.json", options)
                .expect("manifest entry");
            let manifest = RuntimeManifest {
                pack_version: "2026.05.1".to_string(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                platform: crate::runtime::paths::current_runtime_platform(),
                payload_profile: "release".to_string(),
                release_injection_required: false,
                external_artifacts_required: vec![],
                python_relpath: "python/bin/python3".to_string(),
                uv_relpath: "uv/bin/uv".to_string(),
                python_files: vec![crate::runtime::manifest::ManifestEntry {
                    path: "python/bin/python3".to_string(),
                    sha256: format!("{:x}", Sha256::digest(b"python")),
                    size: 6,
                    executable: !cfg!(windows),
                }],
                uv_files: vec![],
                script_files: vec![],
                wheelhouse: vec![],
                caches: vec![],
                native_assets: vec![],
            };
            zip.write_all(&serde_json::to_vec_pretty(&manifest).expect("manifest json"))
                .expect("manifest bytes");
            zip.finish().expect("finish zip");
        }
        let archive_bytes = buffer.into_inner();
        let (release, public_key) = build_signed_release(&archive_bytes);

        let error = download_and_activate_remote_runtime_with_fetch(
            "https://example.com/runtime/bootstrap.json",
            &release,
            app_data_dir.path(),
            &public_key,
            |_| Ok(Cursor::new(archive_bytes.clone())),
            &mut |_| {},
        )
        .expect_err("missing extracted file must fail");

        assert!(error.contains("missing") || error.contains("Missing"));
    }

    #[test]
    fn writes_resume_metadata_after_download() {
        let app_data_dir = tempfile::tempdir().expect("app data dir");
        let archive_bytes = runtime_archive_bytes();
        let (release, public_key) = build_signed_release(&archive_bytes);

        let outcome = download_and_activate_remote_runtime_with_fetch(
            "https://example.com/runtime/bootstrap.json",
            &release,
            app_data_dir.path(),
            &public_key,
            |_| Ok(Cursor::new(archive_bytes.clone())),
            &mut |_| {},
        )
        .expect("download should succeed");
        let metadata_path = bootstrap_download_plan_paths(
            app_data_dir.path(),
            &release.pack_version,
            "runtime-pack.zip",
        )
        .resume_metadata_path;

        let metadata: BootstrapResumeMetadata =
            serde_json::from_slice(&fs::read(&metadata_path).expect("read metadata"))
                .expect("parse metadata");

        assert_eq!(metadata.archive_url, release.archive_url);
        assert_eq!(metadata.downloaded_bytes, release.archive_size);
        assert_eq!(
            outcome.managed_root,
            app_data_dir.path().join("runtime").join("2026.05.1")
        );
    }
}
