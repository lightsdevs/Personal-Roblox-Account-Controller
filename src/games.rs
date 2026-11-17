use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SavedGame {
    pub place_id: u64,
    pub title: String,
}

pub fn load(path: &Path) -> Result<Vec<SavedGame>, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let games: Vec<SavedGame> = serde_json::from_slice(&bytes)
                .map_err(|_| "Saved games file is invalid. It has not been overwritten.")?;
            let mut ids = std::collections::HashSet::new();
            if games
                .iter()
                .any(|g| g.place_id == 0 || g.title.trim().is_empty() || !ids.insert(g.place_id))
            {
                return Err("Saved games file contains invalid or duplicate games. It has not been overwritten.".into());
            }
            Ok(games)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err("Could not read saved games.".into()),
    }
}

pub fn save(path: &Path, games: &[SavedGame]) -> Result<(), String> {
    let directory = path.parent().ok_or("Invalid saved games location.")?;
    std::fs::create_dir_all(directory).map_err(|_| "Could not create saved games directory.")?;
    let bytes = serde_json::to_vec_pretty(games).map_err(|_| "Could not encode saved games.")?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Could not create saved games file.")?;
    file.write_all(&bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Could not write saved games.")?;
    file.persist(path)
        .map_err(|_| "Could not replace saved games file.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_games_survive_restart_removal_and_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("games.json");
        assert!(load(&path).unwrap().is_empty());
        let games = vec![SavedGame {
            place_id: 42,
            title: "Game title 🎮".into(),
        }];
        save(&path, &games).unwrap();
        assert_eq!(load(&path).unwrap(), games);
        save(&path, &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
        for invalid in [
            "invalid",
            r#"[{"place_id":0,"title":"Game"}]"#,
            r#"[{"place_id":42,"title":" "}]"#,
            r#"[{"place_id":42,"title":"A"},{"place_id":42,"title":"B"}]"#,
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
        }
    }
}
