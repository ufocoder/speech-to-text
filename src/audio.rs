mod clip;
mod player;
mod recording;

pub use clip::Audio;
pub use player::Player;
pub use recording::Recording;

pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "ogg", "vorbis", "m4a"];

pub fn is_supported(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}
