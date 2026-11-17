use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
// Never derive Debug: this type contains a credential. Only public metadata crosses IPC.
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub cookie: String,
    pub description: String,
}
pub fn location() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable.")?;
    Ok(PathBuf::from(base)
        .join("PersonalRobloxAccountController")
        .join("accounts.dpapi"))
}
pub fn load(path: &Path) -> Result<Vec<Account>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("Could not read the encrypted account store.".into()),
    };
    let plain = crate::platform::protect(&bytes, false)?;
    serde_json::from_slice(&plain)
        .map_err(|_| "The account store is invalid. It has not been overwritten.".into())
}
pub fn save(path: &Path, accounts: &[Account]) -> Result<(), String> {
    let plain = serde_json::to_vec(accounts).map_err(|_| "Could not encode accounts.")?;
    let encrypted = crate::platform::protect(&plain, true)?;
    let directory = path.parent().ok_or("Invalid store location.")?;
    std::fs::create_dir_all(directory).map_err(|_| "Could not create the account directory.")?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Could not create an encrypted temporary file.")?;
    file.write_all(&encrypted)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Could not write the encrypted account store.")?;
    file.persist(path)
        .map_err(|_| "Could not replace the account store. Previous accounts are unchanged.")?;
    Ok(())
}
pub fn import_reader(
    reader: impl std::io::Read,
    accounts: &mut Vec<Account>,
) -> Result<usize, String> {
    let mut csv = csv::Reader::from_reader(reader);
    let headers = csv.headers().map_err(|_| "Could not read CSV headers.")?;
    if headers.iter().collect::<Vec<_>>() != ["username", "cookie", "description"] {
        return Err("Expected CSV columns: username,cookie,description.".into());
    }
    // Parse all rows before mutation so malformed files cannot cause partial imports.
    let rows: Vec<Account> = csv
        .deserialize()
        .collect::<Result<_, _>>()
        .map_err(|_| "The CSV contains an invalid account row.")?;
    if rows
        .iter()
        .any(|a| a.username.trim().is_empty() || a.cookie.trim().is_empty())
    {
        return Err("The CSV contains an empty username or session.".into());
    }
    let mut count = 0;
    for account in rows {
        if !accounts
            .iter()
            .any(|a| a.username.eq_ignore_ascii_case(&account.username))
        {
            accounts.push(account);
            count += 1;
        }
    }
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_quotes_empty_descriptions_and_duplicates() {
        let mut accounts = Vec::new();
        let csv = "username,cookie,description\r\nTest,fake,\r\nOther,fake,\"line 1,\nline 2\"\r\ntest,fake,duplicate\r\n";
        assert_eq!(import_reader(csv.as_bytes(), &mut accounts).unwrap(), 2);
        assert_eq!(accounts[0].description, "");
        assert_eq!(accounts[1].description, "line 1,\nline 2");
    }
    #[test]
    fn malformed_import_is_atomic() {
        let mut accounts = Vec::new();
        assert!(import_reader(
            b"username,cookie,description\nTest,fake,ok\nbroken\n".as_slice(),
            &mut accounts
        )
        .is_err());
        assert!(accounts.is_empty());
    }
    #[test]
    fn encrypted_store_roundtrip_and_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.dpapi");
        let accounts = vec![Account {
            username: "Test".into(),
            cookie: "fake-cookie".into(),
            description: "a\nb".into(),
        }];
        save(&path, &accounts).unwrap();
        assert_eq!(load(&path).unwrap()[0].description, "a\nb");
        save(&path, &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
    }
}
