//! Private fixture destinations; no arbitrary path reaches the production transfer.
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};

pub(super) struct Directory {
    root: PathBuf,
    downloads: PathBuf,
    nonce: String,
    root_handle: File,
    downloads_handle: File,
    root_identity: (u64, u64),
    downloads_identity: (u64, u64),
}

fn identity(file: &File) -> Result<(u64, u64), String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, out: *mut u32) -> i32;
        }
        // Same thirteen-DWORD layout used by the existing protected fixture.
        let mut info = [0u32; 13];
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err("workflow directory identity unavailable".into());
        }
        Ok((
            u64::from(info[7]),
            (u64::from(info[11]) << 32) | u64::from(info[12]),
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().map_err(|_| "directory identity")?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err("unsupported workflow directory identity".into())
    }
}

fn unlinked(path: &Path, directory: bool) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "workflow path metadata")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("workflow reparse path".into());
        }
    }
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("workflow path type".into());
    }
    Ok(())
}

fn plain(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}

fn open_directory(path: &Path) -> Result<File, String> {
    unlinked(path, true)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_BACKUP_SEMANTICS opens a directory. Excluding FILE_SHARE_DELETE
        // holds its identity through this one process; ordinary reads/writes remain shared.
        options.custom_flags(0x02000000).share_mode(3);
    }
    options
        .open(path)
        .map_err(|_| "workflow directory handle".into())
}

pub(super) fn expected_leaf(attempt: usize, nonce: &str) -> Option<String> {
    match attempt {
        2 => Some(format!("preserve-{nonce}.bin")),
        3 => Some(format!("saved-{nonce}.bin")),
        _ => None,
    }
}

impl Directory {
    pub(super) fn capture(root: PathBuf, nonce: String) -> Result<Self, String> {
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !plain(&root)
            || root.file_name().and_then(|n| n.to_str())
                != Some(format!("original-save-workflow-{nonce}").as_str())
        {
            return Err("workflow private directory/nonce".into());
        }
        for parent in root.ancestors() {
            unlinked(parent, true)?;
        }
        let repository_tmp = std::env::current_dir()
            .map_err(|_| "workflow cwd")?
            .join("tmp")
            .canonicalize()
            .map_err(|_| "workflow repository tmp")?;
        if root
            .parent()
            .ok_or("workflow parent")?
            .canonicalize()
            .map_err(|_| "workflow parent")?
            != repository_tmp
        {
            return Err("workflow directory outside repository tmp".into());
        }
        let downloads = root.join("downloads");
        let root_handle = open_directory(&root)?;
        let downloads_handle = open_directory(&downloads)?;
        let root_identity = identity(&root_handle)?;
        let downloads_identity = identity(&downloads_handle)?;
        Ok(Self {
            root,
            downloads,
            nonce,
            root_handle,
            downloads_handle,
            root_identity,
            downloads_identity,
        })
    }

    pub(super) fn validate(&self, path: &Path, attempt: usize) -> Result<&'static str, String> {
        if !plain(path)
            || path.parent() != Some(self.downloads.as_path())
            || path.file_name().and_then(|n| n.to_str())
                != expected_leaf(attempt, &self.nonce).as_deref()
        {
            return Err("workflow returned path is not an allowed destination".into());
        }
        for parent in self.root.ancestors() {
            unlinked(parent, true)?;
        }
        let current_root = open_directory(&self.root)?;
        let current_downloads = open_directory(&self.downloads)?;
        if identity(&self.root_handle)? != self.root_identity
            || identity(&current_root)? != self.root_identity
            || identity(&self.downloads_handle)? != self.downloads_identity
            || identity(&current_downloads)? != self.downloads_identity
            || path
                .parent()
                .ok_or("workflow destination parent")?
                .canonicalize()
                .map_err(|_| "workflow destination parent")?
                != self
                    .downloads
                    .canonicalize()
                    .map_err(|_| "workflow downloads")?
        {
            return Err("workflow destination directory changed".into());
        }
        match attempt {
            2 => {
                unlinked(path, false)?;
                Ok("existing")
            }
            3 => match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("new"),
                _ => Err("workflow new destination is not absent".into()),
            },
            _ => Err("workflow destination on cancellation attempt".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_leaf_allowlist_is_exact_and_has_no_cancel_destination() {
        let nonce = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            expected_leaf(2, nonce).as_deref(),
            Some("preserve-0123456789abcdef0123456789abcdef.bin")
        );
        assert_eq!(
            expected_leaf(3, nonce).as_deref(),
            Some("saved-0123456789abcdef0123456789abcdef.bin")
        );
        for attempt in [0, 1, 4, usize::MAX] {
            assert!(expected_leaf(attempt, nonce).is_none());
        }
        assert!(!plain(Path::new("relative.bin")));
        #[cfg(windows)]
        assert!(!plain(Path::new(r"C:\owned\..\other.bin")));
    }

    #[test]
    fn returned_paths_require_private_parent_existing_sentinel_and_absent_new_leaf() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nonce = "0123456789abcdef0123456789abcdef".to_owned();
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "sp-workflow-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let downloads = root.join("downloads");
        std::fs::create_dir(&downloads).unwrap();
        let root_handle = open_directory(&root).unwrap();
        let downloads_handle = open_directory(&downloads).unwrap();
        let directory = Directory {
            root: root.clone(),
            downloads: downloads.clone(),
            nonce: nonce.clone(),
            root_identity: identity(&root_handle).unwrap(),
            downloads_identity: identity(&downloads_handle).unwrap(),
            root_handle,
            downloads_handle,
        };
        let existing = downloads.join(expected_leaf(2, &nonce).unwrap());
        let new = downloads.join(expected_leaf(3, &nonce).unwrap());
        assert!(directory.validate(&existing, 2).is_err());
        std::fs::write(&existing, b"sentinel").unwrap();
        assert_eq!(directory.validate(&existing, 2).unwrap(), "existing");
        assert_eq!(directory.validate(&new, 3).unwrap(), "new");
        assert!(
            directory
                .validate(&root.join(expected_leaf(3, &nonce).unwrap()), 3)
                .is_err()
        );
        assert!(
            directory
                .validate(&downloads.join("arbitrary.bin"), 3)
                .is_err()
        );
        assert!(directory.validate(&existing, 1).is_err());
        std::fs::write(&new, b"already exists").unwrap();
        assert!(directory.validate(&new, 3).is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"sentinel");
        drop(directory);
        std::fs::remove_file(existing).unwrap();
        std::fs::remove_file(new).unwrap();
        std::fs::remove_dir(downloads).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
