use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Manifest {
    revision: String,
    assets: Vec<RemoteAsset>,
}

#[derive(Serialize, Deserialize)]
struct RemoteAsset {
    name: String,
    size: u64,
    sha256: String,
}

fn manifest_path(model: SpeechModel, cache: &Path, active: bool) -> PathBuf {
    cache.join(format!(
        "{}.{}.json",
        model.id(),
        if active { "active" } else { "latest" }
    ))
}

fn read(model: SpeechModel, cache: &Path, active: bool) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(manifest_path(model, cache, active)).ok()?).ok()
}

fn save(model: SpeechModel, cache: &Path, active: bool, manifest: &Manifest) -> Result<()> {
    std::fs::create_dir_all(cache)?;
    let mut file = tempfile::NamedTempFile::new_in(cache)?;
    serde_json::to_writer(&mut file, manifest)?;
    file.as_file().sync_all()?;
    file.persist(manifest_path(model, cache, active))?;
    Ok(())
}

fn directory(model: SpeechModel, cache: &Path, manifest: &Manifest) -> PathBuf {
    model
        .source()
        .cache_directory(cache)
        .parent()
        .unwrap()
        .join(&manifest.revision)
}

pub(super) fn active(model: SpeechModel, cache: &Path) -> Option<PathBuf> {
    let manifest = read(model, cache, true)?;
    let directory = directory(model, cache, &manifest);
    manifest
        .assets
        .iter()
        .all(|asset| {
            Asset {
                name: &asset.name,
                size: asset.size,
                sha256: &asset.sha256,
            }
            .is_cached(&directory)
        })
        .then_some(directory)
}

pub(super) fn available(model: SpeechModel, cache: &Path) -> bool {
    let Some(latest) = read(model, cache, false) else {
        return false;
    };
    if let Some(current) = read(model, cache, true).filter(|_| active(model, cache).is_some()) {
        return latest.assets.iter().any(|asset| {
            !current.assets.iter().any(|old| {
                old.name == asset.name && old.size == asset.size && old.sha256 == asset.sha256
            })
        });
    }
    latest.assets.iter().any(|asset| {
        !model.source().assets.iter().any(|old| {
            old.name == asset.name && old.size == asset.size && old.sha256 == asset.sha256
        })
    })
}

pub(super) fn check(model: SpeechModel, cache: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct Info {
        sha: String,
        siblings: Vec<File>,
    }
    #[derive(Deserialize)]
    struct File {
        rfilename: String,
        lfs: Option<Lfs>,
    }
    #[derive(Deserialize)]
    struct Lfs {
        size: u64,
        sha256: String,
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let repo = model.repository();
    let mut response = agent
        .get(format!("https://huggingface.co/api/models/{repo}"))
        .query("blobs", "true")
        .call()?;
    let info: Info = serde_json::from_reader(response.body_mut().as_reader().take(4_000_000))?;
    ensure!(
        info.sha.len() == 40 && info.sha.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid model revision"
    );
    let mut assets = Vec::new();
    for expected in model.source().assets {
        let file = info
            .siblings
            .iter()
            .find(|file| file.rfilename == expected.name)
            .context("Updated model has incompatible filenames")?;
        let (size, sha256) = if let Some(lfs) = &file.lfs {
            (lfs.size, lfs.sha256.clone())
        } else {
            let mut response = agent
                .get(format!(
                    "https://huggingface.co/{repo}/resolve/{}/{}",
                    info.sha, expected.name
                ))
                .call()?;
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(4_000_001)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 4_000_000, "Model metadata file is too large");
            (
                bytes.len() as u64,
                Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            )
        };
        ensure!(
            size > 0 && sha256.len() == 64 && sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid model file metadata"
        );
        assets.push(RemoteAsset {
            name: expected.name.into(),
            size,
            sha256,
        });
    }
    save(
        model,
        cache,
        false,
        &Manifest {
            revision: info.sha,
            assets,
        },
    )
}

pub(super) fn install(
    model: SpeechModel,
    cache: &Path,
    report: impl Fn(u64, u64),
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let manifest = read(model, cache, false).context("No model update metadata")?;
    let directory = directory(model, cache, &manifest);
    std::fs::create_dir_all(&directory)?;
    let total = manifest.assets.iter().map(|asset| asset.size).sum();
    let mut completed = 0;
    for remote in &manifest.assets {
        let asset = Asset {
            name: &remote.name,
            size: remote.size,
            sha256: &remote.sha256,
        };
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            model.repository(),
            manifest.revision,
            asset.name
        );
        asset.download(&url, &directory, &directory.join(asset.name), |bytes| {
            report(completed + bytes, total)
        })?;
        completed += asset.size;
    }
    validate(&directory)?;
    let previous_path = cache.join(format!("{}.previous.json", model.id()));
    if let Some(previous) = read(model, cache, true) {
        let mut file = tempfile::NamedTempFile::new_in(cache)?;
        serde_json::to_writer(&mut file, &previous)?;
        file.persist(&previous_path)?;
    }
    save(model, cache, true, &manifest)?;
    Ok(directory)
}

pub(super) fn remove_active(model: SpeechModel, cache: &Path) -> std::io::Result<()> {
    if let Some(manifest) = read(model, cache, true) {
        let directory = directory(model, cache, &manifest);
        for asset in manifest.assets {
            match std::fs::remove_file(directory.join(asset.name)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                result => result?,
            }
        }
        std::fs::remove_file(manifest_path(model, cache, true))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_repository_changes_when_model_files_are_unchanged() -> Result<()> {
        let cache = tempfile::tempdir()?;
        let model = SpeechModel::Tone;
        let asset = &model.source().assets[0];
        let mut manifest = Manifest {
            revision: "a".repeat(40),
            assets: vec![RemoteAsset {
                name: asset.name.into(),
                size: asset.size,
                sha256: asset.sha256.into(),
            }],
        };
        save(model, cache.path(), false, &manifest)?;
        assert!(!available(model, cache.path()));
        manifest.assets[0].sha256 = "b".repeat(64);
        save(model, cache.path(), false, &manifest)?;
        assert!(available(model, cache.path()));
        assert!(active(model, cache.path()).is_none());
        Ok(())
    }
}

pub(super) fn rollback(
    model: SpeechModel,
    cache: &Path,
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let previous = cache.join(format!("{}.previous.json", model.id()));
    if previous.is_file() {
        let manifest: Manifest = serde_json::from_reader(std::fs::File::open(&previous)?)?;
        let directory = directory(model, cache, &manifest);
        validate(&directory)?;
        save(model, cache, true, &manifest)?;
        std::fs::remove_file(previous)?;
    } else {
        let pinned = model.source().cache_directory(cache);
        validate(&pinned)?;
        let path = manifest_path(model, cache, true);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod rollback_tests {
    use super::*;
    #[test]
    fn failed_validation_preserves_active_revision() -> Result<()> {
        let cache = tempfile::tempdir()?;
        let model = SpeechModel::Tone;
        let manifest = Manifest {
            revision: "a".repeat(40),
            assets: vec![],
        };
        save(model, cache.path(), true, &manifest)?;
        let result = rollback(model, cache.path(), |_| anyhow::bail!("Incompatible model"));
        assert!(result.is_err());
        assert_eq!(
            read(model, cache.path(), true).unwrap().revision,
            manifest.revision
        );
        Ok(())
    }
}
