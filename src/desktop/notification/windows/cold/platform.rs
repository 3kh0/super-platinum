//! Small direct Windows adapters for the cold fixture ledger. No shell commands.
use super::model::{Contents, Executable, Kind, Process, RegistryValue};
use ::windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_INVALID_PARAMETER, FILETIME, HANDLE,
        },
        Security::Cryptography::*,
        Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
        System::{Com::CoTaskMemFree, Registry::*, Threading::*},
        UI::Shell::{
            FOLDERID_LocalAppData, FOLDERID_Programs, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR},
};
use std::{
    fs,
    io::{Read, Write},
    os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

fn known_folder(id: &::windows::core::GUID) -> Result<PathBuf, String> {
    let raw =
        unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.map_err(|e| e.to_string())?;
    let result = PathBuf::from(std::ffi::OsString::from_wide(unsafe { raw.as_wide() }));
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    Ok(result)
}
pub(super) fn root() -> Result<PathBuf, String> {
    let mut path = known_folder(&FOLDERID_LocalAppData)?;
    for component in ["Super Platinum", "notification-fixtures"] {
        path.push(component);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
        safe_path(&path)?;
    }
    Ok(path)
}
pub(super) fn directory(nonce: &str) -> Result<PathBuf, String> {
    if !super::model::valid_nonce(nonce) {
        return Err("Invalid cold fixture nonce".into());
    }
    let path = root()?.join(nonce);
    if path.try_exists().map_err(|e| e.to_string())? {
        safe_path(&path)?;
    }
    Ok(path)
}
pub(super) fn safe_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("Cold fixture path is a reparse point; preserved".into());
    }
    Ok(())
}
/// A share-denying handle serializes adoption and recovery across processes.
/// The file is retained; process death releases the lock without stale-lock repair.
pub(super) fn operation_lock() -> Result<fs::File, String> {
    let path = root()?.join("operation.lock");
    lock_path(&path)
}
fn lock_path(path: &Path) -> Result<fs::File, String> {
    if path.try_exists().map_err(|e| e.to_string())? {
        safe_path(&path)?;
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .custom_flags(0x0020_0000)
        .open(path)
        .map_err(|e| format!("Cold operation is locked or unavailable: {e}"))?;
    if file
        .metadata()
        .map_err(|e| e.to_string())?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err("Cold operation lock is a reparse point; preserved".into());
    }
    Ok(file)
}
pub(super) fn shortcut() -> Result<PathBuf, String> {
    Ok(known_folder(&FOLDERID_Programs)?.join("Super Platinum Notification Fixture.lnk"))
}
pub(super) fn key(kind: Kind) -> Result<String, String> {
    let guid = format!("{{{:?}}}", super::super::identity::FIXTURE_CLSID);
    match kind {
        Kind::Clsid => Ok(format!(r"Software\Classes\CLSID\{guid}")),
        Kind::LocalServer => Ok(format!(r"Software\Classes\CLSID\{guid}\LocalServer32")),
        Kind::AppId => Ok(format!(
            r"Software\Classes\AppUserModelId\{}",
            super::super::identity::FIXTURE
        )),
        Kind::Shortcut => Err("Shortcut is not a registry key".into()),
    }
}
pub(super) fn key_exists(kind: Kind) -> Result<bool, String> {
    let mut handle = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key(kind)?),
            None,
            KEY_READ,
            &mut handle,
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    result.ok().map_err(|e| e.to_string())?;
    unsafe {
        let _ = RegCloseKey(handle);
    }
    Ok(true)
}
pub(super) fn registry_contents(kind: Kind) -> Result<Option<Contents>, String> {
    let mut handle = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key(kind)?),
            None,
            KEY_READ,
            &mut handle,
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    result.ok().map_err(|e| e.to_string())?;
    let result = (|| {
        let (mut subkeys, mut count) = (0, 0);
        unsafe {
            RegQueryInfoKeyW(
                handle,
                None,
                None,
                None,
                Some(&mut subkeys),
                None,
                None,
                Some(&mut count),
                None,
                None,
                None,
                None,
            )
        }
        .ok()
        .map_err(|e| e.to_string())?;
        let owned_child = kind == Kind::Clsid && subkeys == 1 && key_exists(Kind::LocalServer)?;
        if (subkeys != 0 && !owned_child) || count > 4 {
            return Err("Cold registration has unexpected subkeys or values; preserved".into());
        }
        let mut values = Vec::new();
        for index in 0..count {
            let mut name = vec![0u16; 256];
            let mut name_len = 256;
            let mut data = vec![0u8; 32768];
            let mut data_len = 32768;
            let mut kind = 0;
            unsafe {
                RegEnumValueW(
                    handle,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                )
            }
            .ok()
            .map_err(|e| e.to_string())?;
            data.truncate(data_len as usize);
            values.push(RegistryValue {
                name: String::from_utf16(&name[..name_len as usize]).map_err(|e| e.to_string())?,
                kind,
                bytes: data,
            });
        }
        values.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Some(Contents::Registry(values)))
    })();
    unsafe {
        let _ = RegCloseKey(handle);
    }
    result
}
pub(super) fn delete_key(kind: Kind) -> Result<(), String> {
    // Deliberately nonrecursive. A newly added child prevents removal.
    let result = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(key(kind)?)) };
    if result == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        result.ok().map_err(|e| e.to_string())
    }
}
pub(super) fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > super::model::MAX_LEDGER {
        return Err("Cold fixture record too large".into());
    }
    let parent = path.parent().ok_or("Missing cold fixture directory")?;
    safe_path(parent)?;
    if path.exists() {
        safe_path(path)?;
    }
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = parent.join(format!(
        "write-{}-{}-{}.tmp",
        std::process::id(),
        super::super::activation::timestamp_ms(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    unsafe {
        MoveFileExW(
            &HSTRING::from(temporary.as_os_str()),
            &HSTRING::from(path.as_os_str()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}
/// Publish an immutable claim atomically. The fully synced staging inode is
/// linked with create-new semantics, so a crash cannot leave a partial claim.
pub(super) fn exclusive_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing claim parent")?;
    safe_path(parent)?;
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stage = parent.join(format!(
        "claim-{}-{}-{}.json",
        std::process::id(),
        super::super::activation::timestamp_ms(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    atomic_json(&stage, value)?;
    fs::hard_link(&stage, path)
        .map_err(|e| format!("Cold ownership claim already exists or cannot be published: {e}"))?;
    fs::remove_file(stage).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_handle_excludes_adoption_during_recovery_and_releases_on_drop() {
        let directory = std::env::temp_dir().join(format!(
            "super-platinum-cold-lock-test-{}-{}",
            std::process::id(),
            super::super::super::activation::timestamp_ms()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("operation.lock");
        let first = lock_path(&path).unwrap();
        assert!(lock_path(&path).is_err());
        drop(first);
        let next = lock_path(&path).unwrap();
        drop(next);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn immutable_claim_collision_preserves_first_complete_owner() {
        let directory = std::env::temp_dir().join(format!(
            "super-platinum-cold-claim-test-{}-{}",
            std::process::id(),
            super::super::super::activation::timestamp_ms()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("claim.json");
        let first = Process {
            pid: 12,
            created: 99,
        };
        exclusive_json(&path, &first).unwrap();
        assert!(
            exclusive_json(
                &path,
                &Process {
                    pid: 13,
                    created: 100
                }
            )
            .is_err()
        );
        let actual: Process = serde_json::from_slice(&read_bounded(&path, 256).unwrap()).unwrap();
        assert_eq!(actual, first);
        for entry in fs::read_dir(&directory).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        fs::remove_dir(directory).unwrap();
    }
}
pub(super) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    safe_path(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        Err("Cold artifact exceeds size limit".into())
    } else {
        Ok(bytes)
    }
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn process(handle: HANDLE, pid: u32) -> Result<Process, String> {
    let (mut created, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) }
        .map_err(|e| e.to_string())?;
    Ok(Process {
        pid,
        created: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
    })
}
pub(super) fn current_process() -> Result<Process, String> {
    process(unsafe { GetCurrentProcess() }, std::process::id())
}
pub(super) fn alive(owner: &Process) -> Result<bool, String> {
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, owner.pid) } {
        Ok(handle) => OwnedHandle(handle),
        Err(error)
            if error.code() == ::windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(format!("Cannot verify fixture process departure: {error}")),
    };
    if process(handle.0, owner.pid)? != *owner {
        return Ok(false);
    }
    let mut status = 0;
    unsafe { GetExitCodeProcess(handle.0, &mut status) }.map_err(|e| e.to_string())?;
    Ok(status == 259)
}
pub(super) fn executable() -> Result<Executable, String> {
    let path = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let mut algorithm = BCRYPT_ALG_HANDLE::default();
    unsafe {
        BCryptOpenAlgorithmProvider(
            &mut algorithm,
            BCRYPT_SHA256_ALGORITHM,
            PCWSTR::null(),
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    let mut hash = BCRYPT_HASH_HANDLE::default();
    let result = (|| {
        // Windows7+ owns the hash object when no caller buffer is supplied.
        unsafe { BCryptCreateHash(algorithm, &mut hash, None, None, 0) }
            .ok()
            .map_err(|e| e.to_string())?;
        let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            unsafe { BCryptHashData(hash, &buffer[..count], 0) }
                .ok()
                .map_err(|e| e.to_string())?;
        }
        let mut digest = [0; 32];
        unsafe { BCryptFinishHash(hash, &mut digest, 0) }
            .ok()
            .map_err(|e| e.to_string())?;
        let text = path.to_str().ok_or("Executable path is not Unicode")?;
        let normalized = text
            .strip_prefix(r"\\?\")
            .filter(|value| !value.starts_with("UNC\\"))
            .unwrap_or(text);
        Ok(Executable {
            path: normalized.into(),
            sha256: digest.iter().map(|b| format!("{b:02x}")).collect(),
        })
    })();
    unsafe {
        if !hash.0.is_null() {
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(algorithm, 0);
    }
    result
}
