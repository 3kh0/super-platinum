//! Temporary fixture ownership only; no configured production path constructor.
use super::*;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(in crate::drafts::store) struct Owned {
    root: PathBuf,
    path: PathBuf,
    account: String,
    marker: Mutex<File>,
    identity: Identity,
    worker: std::sync::Mutex<Option<String>>,
    reopened: Option<(File, Identity)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity(Vec<u64>);

pub(super) fn identity(file: &File) -> Result<Identity, AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(Identity(vec![m.dev(), m.ino()]))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, out: *mut u32) -> i32;
        }
        // BY_HANDLE_FILE_INFORMATION consists of thirteen DWORDs. The retained
        // handle is valid throughout this synchronous read; no pointer escapes.
        let mut info = [0u32; 13];
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Identity(vec![
            u64::from(info[7]),
            (u64::from(info[11]) << 32) | u64::from(info[12]),
        ]))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err(invalid("unsupported file identity platform"))
    }
}
pub(super) fn plain_chain(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid("noncanonical upgrade path"));
    }
    for component in path.ancestors() {
        let m = std::fs::symlink_metadata(component)?;
        if m.file_type().is_symlink() {
            return Err(invalid("upgrade symlink refused"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err(invalid("upgrade reparse point refused"));
            }
        }
    }
    Ok(())
}
fn retained(path: &Path) -> Result<File, AppError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3); // read/write sharing, deliberately no FILE_SHARE_DELETE
    }
    Ok(options.open(path)?)
}
impl Owned {
    pub(in crate::drafts::store) fn fresh() -> Result<Arc<Self>, AppError> {
        let base = std::fs::canonicalize(std::env::temp_dir())?;
        plain_chain(&base)?;
        let root = base.join(format!("super-platinum-upgrade-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root)?;
        let account = uuid::Uuid::new_v4().to_string();
        let marker_path = root.join("owner");
        let mut options = OpenOptions::new();
        options.write(true).read(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut marker = options.open(&marker_path)?;
        marker
            .try_lock()
            .map_err(|_| invalid("exclusive temporary account lock unavailable"))?;
        marker.write_all(account.as_bytes())?;
        marker.sync_all()?;
        let identity = identity(&marker)?;
        let path = root.join(format!("drafts-{account}.sqlite"));
        Ok(Arc::new(Self {
            root,
            path,
            account,
            marker: Mutex::new(marker),
            identity,
            worker: std::sync::Mutex::new(None),
            reopened: None,
        }))
    }
    pub(in crate::drafts::store) fn account(&self) -> &str {
        &self.account
    }
    pub(in crate::drafts::store) fn path(&self) -> &Path {
        &self.path
    }
    pub(in crate::drafts::store) fn bind_worker(&self, id: &str) -> Result<(), AppError> {
        let mut worker = self.worker.lock().unwrap();
        if worker.as_deref().is_some_and(|old| old != id) {
            return Err(invalid(
                "exclusive account already belongs to another worker",
            ));
        }
        if worker.is_none() {
            *worker = Some(id.into());
        }
        Ok(())
    }
    pub(in crate::drafts::store) fn check(&self) -> Result<(), AppError> {
        plain_chain(&self.root)?;
        // Windows byte-range locks deny reads through another opened handle,
        // including one in this process. Keep the original lock held and read
        // only through its owning handle. The mutex also serializes its cursor.
        let mut marker = self
            .marker
            .lock()
            .map_err(|_| invalid("upgrade marker cursor lock poisoned"))?;
        marker.seek(SeekFrom::Start(0))?;
        let mut marker_bytes = Vec::new();
        (&mut *marker).take(37).read_to_end(&mut marker_bytes)?;
        if self.path.parent() != Some(self.root.as_path())
            || self.path.file_name().and_then(|s| s.to_str())
                != Some(format!("drafts-{}.sqlite", self.account).as_str())
            || !valid_uuid(&self.account)
            || identity(&marker)? != self.identity
            || marker_bytes != self.account.as_bytes()
        {
            return Err(invalid("upgrade owner changed"));
        }
        if let Some((file, expected)) = &self.reopened {
            plain_chain(&self.path)?;
            if identity(file)? != *expected || identity(&retained(&self.path)?)? != *expected {
                return Err(invalid("reopened fixture database replaced"));
            }
        }
        let actual = retained(&self.root.join("owner"))?;
        if identity(&actual)? != self.identity {
            return Err(invalid("upgrade marker replaced"));
        }
        Ok(())
    }
    pub(super) fn retain_db(&self) -> Result<Option<(File, Identity)>, AppError> {
        self.check()?;
        match std::fs::symlink_metadata(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
            Ok(m) => {
                if !m.is_file() {
                    return Err(invalid("upgrade database is not a file"));
                }
                plain_chain(&self.path)?;
                let file = retained(&self.path)?;
                let id = identity(&file)?;
                Ok(Some((file, id)))
            }
        }
    }
    pub(super) fn create_db(&self) -> Result<File, AppError> {
        self.check()?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3);
        }
        Ok(options.open(&self.path)?)
    }
    pub(super) fn readonly_preflight(&self, file: &File) -> Result<(), AppError> {
        // READ_ONLY SQLite can still create WAL shared-memory files. This finite
        // cohort refuses WAL and every pre-existing recovery sidecar before any
        // SQLite open; it neither ignores WAL nor repairs a hot rollback journal.
        use std::io::{Read, Seek, SeekFrom};
        for suffix in ["-wal", "-shm", "-journal"] {
            let path = std::path::PathBuf::from(format!("{}{suffix}", self.path.display()));
            match std::fs::symlink_metadata(path) {
                Ok(_) => return Err(invalid("journal recovery or WAL requires separate review")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        let mut header = [0u8; 100];
        let mut reader = file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        reader.read_exact(&mut header)?;
        if &header[..16] != b"SQLite format 3\0" || header[18] != 1 || header[19] != 1 {
            return Err(invalid(
                "only a closed rollback-mode SQLite file may be inspected",
            ));
        }
        Ok(())
    }
}
// No Drop deletion: failed test evidence is retained. No user/config root can be
// supplied, and no account UUID is accepted from a caller. Future production
// construction requires a separately reviewed account-wide coordinator.

#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn m02_original_locked_handle_remains_readable_and_exclusive() {
        let owner = Owned::fresh().unwrap();
        owner.check().unwrap();
        owner.check().unwrap();
        let path = owner.root.join("owner");
        let mut other = retained(&path).unwrap();
        assert_eq!(identity(&other).unwrap(), owner.identity);
        assert!(other.try_lock().is_err());
        #[cfg(windows)]
        {
            let error = other.read(&mut [0u8; 1]).unwrap_err();
            assert_eq!(error.raw_os_error(), Some(33));
            assert!(std::fs::rename(&path, path.with_extension("moved")).is_err());
        }
        #[cfg(not(windows))]
        let _ = &mut other;
        drop(other);
        // Concurrent checks must not share an unprotected seek/read cursor.
        let readers = (0..4)
            .map(|_| {
                let owner = owner.clone();
                std::thread::spawn(move || {
                    for _ in 0..8 {
                        owner.check().unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for reader in readers {
            reader.join().unwrap();
        }
        owner.check().unwrap();
    }

    #[test]
    fn m02_locked_marker_tampering_and_extra_bytes_still_refuse() {
        for length in [0, 36, 37] {
            let owner = Owned::fresh().unwrap();
            {
                // Mutation through the actual lock owner is fixture injection;
                // the ownership check must still detect changed retained bytes.
                let mut marker = owner.marker.lock().unwrap();
                marker.seek(SeekFrom::Start(0)).unwrap();
                marker.set_len(0).unwrap();
                let mut bytes = owner.account.as_bytes().to_vec();
                if length == 0 {
                    bytes.clear();
                }
                if length == 36 {
                    bytes[0] = if bytes[0] == b'0' { b'1' } else { b'0' };
                }
                if length == 37 {
                    bytes.push(b'!');
                }
                marker.write_all(&bytes).unwrap();
                marker.sync_all().unwrap();
            }
            assert!(owner.check().is_err(), "length {length}");
        }
    }
}

#[path = "upgrade_reopen.rs"]
mod reopen;
