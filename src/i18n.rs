use rust_i18n::t;
use std::io::{self, Write};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
}

impl Language {
    pub fn load() -> Self {
        match std::fs::read_to_string(crate::storage::language_file()).as_deref() {
            Ok("en") => Self::English,
            _ => Self::Russian,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Russian => "ru",
            Self::English => "en",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Russian => t!("language_ru").into_owned(),
            Self::English => t!("language_en").into_owned(),
        }
    }

    pub fn activate(self) {
        gpui_kit::component::set_locale(self.code());
    }

    pub fn save(self) -> io::Result<()> {
        let path = crate::storage::language_file();
        let parent = path.parent().expect("language preference directory");
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(self.code().as_bytes())?;
        file.persist(path)?;
        Ok(())
    }
}

pub enum Message {
    Empty,
    Text(&'static str),
    Detail(&'static str, String),
    Error(String),
}

impl Message {
    pub fn render(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(key) => t!(*key).into_owned(),
            Self::Detail(key, value) => t!(*key, detail = value).into_owned(),
            Self::Error(error) => error.clone(),
        }
    }
}
