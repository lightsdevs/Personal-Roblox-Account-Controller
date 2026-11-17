use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE},
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
    System::Threading::CreateMutexW,
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub struct MutexGuard(HANDLE);
impl MutexGuard {
    pub fn create(name: &str, exclusive: bool) -> Result<Self, String> {
        let name = wide(name);
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("Could not create the Windows mutex.".into());
        }
        let guard = Self(handle);
        if exclusive && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err("The Rust controller is already running.".into());
        }
        Ok(guard)
    }
}
impl Drop for MutexGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| "Account store is too large.")?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // DPAPI binds the store to the current Windows user, without machine-wide access.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err("Windows could not encrypt/decrypt the account store for this user.".into());
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
pub fn launch(uri: &str) -> Result<(), String> {
    let uri = wide(uri);
    let operation = wide("open");
    // Direct protocol invocation: no shell interpretation and no ticket in diagnostics.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            uri.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err("Windows could not launch Roblox. Check that Roblox is installed.".into())
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn dpapi_roundtrip_and_corruption() {
        let plain = b"synthetic-test-data";
        let encrypted = super::protect(plain, true).unwrap();
        assert_ne!(encrypted, plain);
        assert_eq!(super::protect(&encrypted, false).unwrap(), plain);
        assert!(super::protect(b"invalid", false).is_err());
    }
}
