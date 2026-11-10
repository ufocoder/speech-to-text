use anyhow::{Context, Result, ensure};
use rust_i18n::t;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpeechModel {
    Tone,
    GigaAm,
    Parakeet,
}

struct Asset {
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

struct Source {
    repo: &'static str,
    revision: &'static str,
    assets: &'static [Asset],
}

impl SpeechModel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tone => "T-one",
            Self::GigaAm => "GigaAM-v3",
            Self::Parakeet => "Parakeet v3 INT8",
        }
    }

    fn source(self) -> Source {
        match self {
            Self::Tone => Source {
                repo: "t-tech/T-one",
                revision: "106f3b0b32a9e107eb613312e4ebc61ff3d53926",
                assets: &[Asset {
                    name: "model.onnx",
                    size: 144193371,
                    sha256: "707e4a282d5036304a0b8603ad83945840c3075f53c49911ef88c19bcc5a7d52",
                }],
            },
            Self::GigaAm => Source {
                repo: "nocmt/GigaAM-v3-E2E-CTC-int8-ONNX",
                revision: "0928919f7df949871b507e16497b9e8cff7d0c6b",
                assets: &[
                    Asset {
                        name: "model.int8.onnx",
                        size: 224893668,
                        sha256: "784d77aaa7a0899a247e72c71d667792087ee4dec985db1f57961767a5f6014f",
                    },
                    Asset {
                        name: "tokens.txt",
                        size: 2007,
                        sha256: "142de7570b3de5b3035ce111a89c228e80e6085273731d944093ddf24fa539cd",
                    },
                ],
            },
            Self::Parakeet => Source {
                repo: "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
                revision: "876ff91b4ab4b89c328afdb2b27ff879d3e42f87",
                assets: &[
                    Asset {
                        name: "encoder.int8.onnx",
                        size: 652184281,
                        sha256: "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247",
                    },
                    Asset {
                        name: "decoder.int8.onnx",
                        size: 11845275,
                        sha256: "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e",
                    },
                    Asset {
                        name: "joiner.int8.onnx",
                        size: 6355277,
                        sha256: "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3",
                    },
                    Asset {
                        name: "tokens.txt",
                        size: 93939,
                        sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
                    },
                ],
            },
        }
    }

    pub fn cached_directory(self, cache: &Path) -> Option<PathBuf> {
        let source = self.source();
        let directory = source.cache_directory(cache);
        source
            .assets
            .iter()
            .all(|asset| asset.is_cached(&directory))
            .then_some(directory)
    }

    pub fn prepare(self, cache: &Path) -> Result<PathBuf> {
        let source = self.source();
        let directory = source.cache_directory(cache);
        for asset in source.assets {
            let path = directory.join(asset.name);
            if asset.is_cached(&directory) {
                continue;
            }
            std::fs::create_dir_all(&directory)?;
            let base = std::env::var("SPEECH_MODEL_BASE_URL")
                .unwrap_or_else(|_| "https://huggingface.co".into());
            let url = format!(
                "{}/{}/resolve/{}/{}",
                base.trim_end_matches('/'),
                source.repo,
                source.revision,
                asset.name
            );
            asset.download(&url, &directory, &path).with_context(|| {
                t!("download_failed", model = self.label(), file = asset.name).into_owned()
            })?;
        }
        Ok(directory)
    }
}

impl Source {
    fn cache_directory(&self, cache: &Path) -> PathBuf {
        cache
            .join(format!("models--{}", self.repo.replace('/', "--")))
            .join("snapshots")
            .join(self.revision)
    }
}

impl Asset {
    fn is_cached(&self, directory: &Path) -> bool {
        directory
            .join(self.name)
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.len() == self.size)
    }

    fn download(&self, url: &str, directory: &Path, path: &Path) -> Result<()> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(30 * 60)))
            .build()
            .into();
        let mut response = agent.get(url).call()?;
        let mut reader = response.body_mut().as_reader().take(self.size + 1);
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        let mut digest = Sha256::new();
        let mut length = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count])?;
            length += count as u64;
        }
        ensure!(length == self.size, t!("model_size_mismatch").into_owned());
        ensure!(
            digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                == self.sha256,
            t!("model_checksum_mismatch").into_owned()
        );
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }
}
