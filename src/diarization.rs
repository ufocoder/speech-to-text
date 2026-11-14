use crate::audio::Audio;
use anyhow::{Context, Result, ensure};
use rust_i18n::t;
use sherpa_onnx::{
    OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig, OfflineSpeakerDiarizationSegment,
    SpeakerEmbeddingExtractor, SpeakerEmbeddingManager,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn split(
    audio: &Audio,
    directory: &Path,
    segmentation_file: &str,
    report: impl Fn(u8) -> Result<()>,
) -> Result<Vec<OfflineSpeakerDiarizationSegment>> {
    let segmentation = directory.join(segmentation_file);
    let embedding = directory.join("3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx");
    ensure!(
        segmentation.is_file() && embedding.is_file(),
        t!("speaker_missing_models")
    );
    let mut config = OfflineSpeakerDiarizationConfig::default();
    config.segmentation.pyannote.model = Some(segmentation.to_string_lossy().into_owned());
    config.embedding.model = Some(embedding.to_string_lossy().into_owned());
    let model = OfflineSpeakerDiarization::create(&config)
        .context(t!("speaker_model_failed").into_owned())?;
    let rate = model.sample_rate();
    ensure!(rate > 0, t!("speaker_model_failed"));
    let extractor = SpeakerEmbeddingExtractor::create(&config.embedding)
        .context(t!("speaker_model_failed").into_owned())?;
    let voices = SpeakerEmbeddingManager::create(extractor.dim())
        .context(t!("speaker_model_failed").into_owned())?;
    let mut next_speaker = 0;
    let mut result = Vec::new();
    report(0)?;
    let chunk_size = rate as usize * 60;

    for (index, samples) in audio.mono_chunks(rate as u32, chunk_size).enumerate() {
        let mut segments = model
            .process(&samples)
            .context(t!("speaker_model_failed").into_owned())?
            .sort_by_start_time();
        segments.retain(valid_segment);
        let mut representatives = BTreeMap::new();
        for segment in &segments {
            let best = representatives.entry(segment.speaker).or_insert(segment);
            if segment.end - segment.start > best.end - best.start {
                *best = segment;
            }
        }

        let mut labels = BTreeMap::new();
        let mut used = BTreeSet::new();
        for (local, segment) in representatives {
            let start = ((segment.start * rate as f32) as usize).min(samples.len());
            let end = ((segment.end * rate as f32) as usize).min(samples.len());
            let embedding = if end > start {
                extractor.create_stream().and_then(|stream| {
                    stream.accept_waveform(rate, &samples[start..end]);
                    stream.input_finished();
                    extractor
                        .is_ready(&stream)
                        .then(|| extractor.compute(&stream))
                        .flatten()
                })
            } else {
                None
            };
            let previous = embedding.as_ref().and_then(|vector| {
                voices
                    .search(vector, 0.6)
                    .and_then(|name| name.parse::<i32>().ok())
                    .filter(|speaker| !used.contains(speaker))
            });
            let global = previous.unwrap_or_else(|| {
                let speaker = next_speaker;
                next_speaker += 1;
                if let Some(vector) = &embedding {
                    voices.add(&speaker.to_string(), vector);
                }
                speaker
            });
            used.insert(global);
            labels.insert(local, global);
        }

        let offset = index as f32 * 60.;
        let duration = samples.len() as f32 / rate as f32;
        for mut segment in segments {
            if segment.start >= duration {
                continue;
            }
            segment.speaker = labels[&segment.speaker];
            segment.start += offset;
            segment.end = offset + segment.end.min(duration);
            result.push(segment);
        }
        report(
            ((f64::from(offset) + f64::from(duration)) * 100. / audio.duration()).min(99.) as u8,
        )?;
    }
    Ok(result)
}

fn valid_segment(segment: &OfflineSpeakerDiarizationSegment) -> bool {
    segment.start.is_finite()
        && segment.end.is_finite()
        && segment.start >= 0.
        && segment.end > segment.start
        && segment.speaker >= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_overlapping_speakers_and_rejects_invalid_segments() {
        let segment = |start, end, speaker| OfflineSpeakerDiarizationSegment {
            start,
            end,
            speaker,
            confidence: 0.,
        };
        let mut segments = [
            segment(0., 2., 0),
            segment(1., 3., 1),
            segment(f32::NAN, 2., 0),
            segment(1., f32::INFINITY, 0),
            segment(-1., 2., 0),
            segment(2., 2., 0),
            segment(3., 2., 0),
            segment(0., 2., -1),
        ]
        .to_vec();
        segments.retain(valid_segment);
        assert_eq!(
            segments
                .iter()
                .map(|s| (s.start, s.end, s.speaker))
                .collect::<Vec<_>>(),
            vec![(0., 2., 0), (1., 3., 1)]
        );
    }
}
