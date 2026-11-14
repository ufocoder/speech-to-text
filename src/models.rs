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
    WhisperTurbo,
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

pub(crate) struct WhisperOnnxFiles {
    pub encoder: &'static str,
    pub decoder: &'static str,
    pub tokens: &'static str,
}

const WHISPER_TURBO_FILES: WhisperOnnxFiles = WhisperOnnxFiles {
    encoder: "turbo-encoder.int8.onnx",
    decoder: "turbo-decoder.int8.onnx",
    tokens: "turbo-tokens.txt",
};

impl SpeechModel {
    pub const ALL: [Self; 4] = [Self::Tone, Self::GigaAm, Self::Parakeet, Self::WhisperTurbo];

    pub fn id(self) -> &'static str {
        match self {
            Self::Tone => "t-one",
            Self::GigaAm => "gigaam-v3",
            Self::Parakeet => "parakeet-v3-int8",
            Self::WhisperTurbo => "whisper-large-v3-turbo-int8",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tone => "T-one",
            Self::GigaAm => "GigaAM-v3",
            Self::Parakeet => "Parakeet v3 INT8",
            Self::WhisperTurbo => "Whisper Large v3 Turbo INT8",
        }
    }

    pub(crate) fn whisper_onnx_files(self) -> Option<WhisperOnnxFiles> {
        match self {
            Self::WhisperTurbo => Some(WHISPER_TURBO_FILES),
            Self::Tone | Self::GigaAm | Self::Parakeet => None,
        }
    }

    pub fn repository(self) -> &'static str {
        self.source().repo
    }

    pub fn parameters_millions(self) -> &'static str {
        match self {
            Self::Tone => "71",
            Self::GigaAm => "220–240",
            Self::Parakeet => "600",
            Self::WhisperTurbo => "809",
        }
    }

    pub fn delete_cached(self, cache: &Path) -> std::io::Result<()> {
        let source = self.source();
        let directory = source.cache_directory(cache);
        for asset in source.assets {
            match std::fs::remove_file(directory.join(asset.name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub fn download_size_mb(self) -> u64 {
        let bytes: u64 = self.source().assets.iter().map(|asset| asset.size).sum();
        (bytes + 500_000) / 1_000_000
    }

    pub fn languages_summary(self) -> String {
        match self {
            Self::Tone | Self::GigaAm => t!("model_languages_russian").into_owned(),
            Self::Parakeet => t!("model_languages_parakeet_summary").into_owned(),
            Self::WhisperTurbo => t!("model_languages_whisper").into_owned(),
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
            Self::WhisperTurbo => Source {
                repo: "cohearo/sherpa-onnx-whisper-large-v3-turbo-attention-int8",
                revision: "2113f27d685916e4963627c86699e3fb1e61470a",
                assets: &[
                    Asset {
                        name: WHISPER_TURBO_FILES.encoder,
                        size: 674716477,
                        sha256: "1944cc0d1c50724b2437887519bae67e9bae48dcee794efbf604db8461c5e50e",
                    },
                    Asset {
                        name: WHISPER_TURBO_FILES.decoder,
                        size: 361152887,
                        sha256: "0f5e075e6d6a4ea709e62b3664ded0b65ed0ce0359b1d83c1eafa3834e73648a",
                    },
                    Asset {
                        name: WHISPER_TURBO_FILES.tokens,
                        size: 816730,
                        sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
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

    pub fn prepare(self, cache: &Path, report: impl Fn(u64, u64)) -> Result<PathBuf> {
        let source = self.source();
        let directory = source.cache_directory(cache);
        let missing: Vec<_> = source
            .assets
            .iter()
            .filter(|asset| !asset.is_cached(&directory))
            .collect();
        let total = missing.iter().map(|asset| asset.size).sum();
        let mut completed = 0;
        for asset in missing {
            let path = directory.join(asset.name);
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
            asset
                .download(&url, &directory, &path, |bytes| {
                    report(completed + bytes, total)
                })
                .with_context(|| {
                    t!("download_failed", model = self.label(), file = asset.name).into_owned()
                })?;
            completed += asset.size;
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

    fn download(
        &self,
        url: &str,
        directory: &Path,
        path: &Path,
        report: impl Fn(u64),
    ) -> Result<()> {
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
            report(length.min(self.size));
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

pub const SPEAKER_MODELS: [(&str, &str, &str, &str); 4] = [
    (
        "pyannote segmentation 3.0",
        "csukuangfj/sherpa-onnx-pyannote-segmentation-3-0",
        "model.onnx",
        "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
    ),
    (
        "3D-Speaker ERes2Net",
        "csukuangfj/speaker-embedding-models",
        "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx",
        "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b",
    ),
    (
        "pyannote segmentation 3.0 INT8",
        "csukuangfj/sherpa-onnx-pyannote-segmentation-3-0",
        "model.int8.onnx",
        "d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d",
    ),
    (
        "Reverb Diarization v1",
        "csukuangfj/sherpa-onnx-reverb-diarization-v1",
        "reverb-v1.onnx",
        "8249e2e323f8fb0566a387d94b40a73d0b75d54ee60d02dcd729a56a5ba8ecea",
    ),
];

pub fn download_speaker_model(index: usize, directory: &Path, report: impl Fn(u8)) -> Result<()> {
    let (_, repo, name, checksum) = SPEAKER_MODELS[index];
    let remote_name = if index == SpeakerModel::Reverb.segmentation_index() {
        "model.onnx"
    } else {
        name
    };
    std::fs::create_dir_all(directory)?;
    let mut response = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .new_agent()
        .get(format!(
            "https://huggingface.co/{repo}/resolve/main/{remote_name}"
        ))
        .call()?;
    let total = response
        .body()
        .content_length()
        .filter(|&length| length > 0);
    let mut reader = response.body_mut().as_reader().take(64_000_001);
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(total) = total {
            report((bytes.len() as u64 * 100 / total).min(99) as u8);
        }
    }
    ensure!(bytes.len() <= 64_000_000, t!("model_size_mismatch"));
    ensure!(
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
            == checksum,
        t!("model_checksum_mismatch")
    );
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    file.persist(directory.join(name))?;
    Ok(())
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SpeakerModel {
    #[default]
    Standard,
    Int8,
    Reverb,
}

impl SpeakerModel {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Int8, Self::Reverb];

    pub fn segmentation_index(self) -> usize {
        match self {
            Self::Standard => 0,
            Self::Int8 => 2,
            Self::Reverb => 3,
        }
    }

    pub fn label(self) -> &'static str {
        SPEAKER_MODELS[self.segmentation_index()].0
    }
}
