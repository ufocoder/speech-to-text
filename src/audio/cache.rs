use super::Audio;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Metadata {
    channels: usize,
    rate: u32,
    peaks: Vec<f32>,
    bytes: usize,
}

pub(super) fn key(source: &Path, report: &impl Fn(u8) -> Result<()>) -> Result<PathBuf> {
    let mut file = File::open(source)?;
    let mut hash = Sha256::new();
    // The version covers PCM encoding and waveform preparation.
    hash.update(b"s2t-pcm-native-i16-v1");
    hash.update(if cfg!(target_endian = "little") {
        b"le"
    } else {
        b"be"
    });
    let mut buffer = [0; 128 * 1024];
    loop {
        report(0)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(crate::storage::audio_cache().join(format!("{digest}.pcm")))
}

pub(super) fn load(path: &Path) -> Result<Audio> {
    let mut file = File::open(path)?;
    let mut length = [0; 8];
    file.read_exact(&mut length)?;
    let length = u64::from_le_bytes(length);
    ensure!(length <= 64_000, "Invalid audio cache header");
    let mut header = vec![0; length as usize];
    file.read_exact(&mut header)?;
    let meta: Metadata = serde_json::from_slice(&header)?;
    ensure!(
        meta.channels > 0 && meta.channels <= 64 && meta.rate > 0,
        "Invalid audio cache format"
    );
    ensure!(
        meta.bytes > 0 && meta.bytes % (meta.channels * 2) == 0,
        "Invalid PCM size"
    );
    ensure!(
        file.metadata()?.len() == 8 + length + meta.bytes as u64,
        "Truncated audio cache"
    );
    ensure!(
        meta.peaks.len() <= 192
            && meta
                .peaks
                .iter()
                .all(|peak| peak.is_finite() && (0. ..=1.).contains(peak)),
        "Invalid waveform cache"
    );
    // Cache files are atomically replaced, never edited while mapped.
    let samples = unsafe {
        memmap2::MmapOptions::new()
            .offset(8 + length)
            .len(meta.bytes)
            .map(&file)?
    };
    Ok(Audio {
        samples,
        channels: meta.channels,
        rate: meta.rate,
        peaks: meta.peaks.into(),
    })
}

pub(super) fn store(path: &Path, audio: &Audio) -> Result<()> {
    let directory = path.parent().unwrap();
    std::fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    let header = serde_json::to_vec(&Metadata {
        channels: audio.channels,
        rate: audio.rate,
        peaks: audio.peaks.to_vec(),
        bytes: audio.samples.len(),
    })?;
    file.write_all(&(header.len() as u64).to_le_bytes())?;
    file.write_all(&header)?;
    file.write_all(&audio.samples)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_pcm_and_waveform_and_rejects_truncated_cache() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("test.wav");
        let mut writer = hound::WavWriter::create(
            &source,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        for value in 0..1600 {
            writer.write_sample(value as i16)?;
        }
        writer.finalize()?;
        let audio = Audio::decode(&source, &|_| Ok(()))?;
        let path = directory.path().join("cache.pcm");
        store(&path, &audio)?;
        let restored = load(&path)?;
        assert_eq!(&*audio.samples, &*restored.samples);
        assert_eq!(audio.peaks, restored.peaks);
        assert_eq!(audio.duration(), restored.duration());
        drop(restored);
        File::options().write(true).open(&path)?.set_len(10)?;
        assert!(load(&path).is_err());
        Ok(())
    }
}
