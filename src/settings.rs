use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub singleton: bool,
    pub dark_mode: bool,
    pub background_cpu_percent: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            singleton: true,
            dark_mode: true,
            background_cpu_percent: 15,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes)
                .ok()
                .filter(|s| (1..=100).contains(&s.background_cpu_percent))
                .ok_or_else(|| "Settings file is invalid. It has not been overwritten.".into()),
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
        std::fs::write(&path, br#"{"dark_mode":false}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().background_cpu_percent, 15);
        Settings {
            singleton: false,
            dark_mode: false,
            background_cpu_percent: 33,
        }
        .save(&path)
        .unwrap();
        let saved = Settings::load(&path).unwrap();
        assert!(!saved.singleton && !saved.dark_mode);
        assert_eq!(saved.background_cpu_percent, 33);
        defaults.save(&path).unwrap();
        assert!(Settings::load(&path).unwrap().singleton);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid");
    }
}
