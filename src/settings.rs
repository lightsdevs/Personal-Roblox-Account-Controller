use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub singleton: bool,
    pub dark_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            singleton: true,
            dark_mode: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "Settings file is invalid. It has not been overwritten.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("Could not read settings.".into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let directory = path.parent().ok_or("Invalid settings location.")?;
        std::fs::create_dir_all(directory).map_err(|_| "Could not create settings directory.")?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "Could not encode settings.")?;
        let mut file = tempfile::NamedTempFile::new_in(directory)
            .map_err(|_| "Could not create settings file.")?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| "Could not write settings.")?;
        file.persist(path)
            .map_err(|_| "Could not replace settings file.")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_survive_restart_and_invalid_files_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let defaults = Settings::load(&path).unwrap();
        assert!(defaults.singleton && defaults.dark_mode);
        Settings {
            singleton: false,
            dark_mode: false,
        }
        .save(&path)
        .unwrap();
        let saved = Settings::load(&path).unwrap();
        assert!(!saved.singleton && !saved.dark_mode);
        defaults.save(&path).unwrap();
        assert!(Settings::load(&path).unwrap().singleton);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid");
    }
}
