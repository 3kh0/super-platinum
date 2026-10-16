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
        2 => Some(format!("overwrite-{nonce}.txt")),
        3 => Some(format!("retire-{nonce}.txt")),
        _ => None,
    }
}

impl Directory {
    fn marker(root: &Path, nonce: &str) -> Result<(), String> {
        use std::io::Read;
        let path = root.join("owner.json");
        unlinked(&path, false)?;
        let file = File::open(path).map_err(|_| "cached marker open")?;
        if file.metadata().map_err(|_| "cached marker metadata")?.len() > 4096 {
            return Err("cached marker too large".into());
        }
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| "cached marker read")?;
        if bytes.len() > 4096 {
            return Err("cached marker grew too large".into());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "cached marker JSON")?;
        if value != serde_json::json!({"version":1,"nonce":nonce,"fixture":super::ALIAS}) {
            return Err("cached marker owner differs".into());
        }
        Ok(())
    }
    pub(super) fn capture(root: PathBuf, nonce: String) -> Result<Self, String> {
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !plain(&root)
            || root.file_name().and_then(|n| n.to_str())
                != Some(format!("cached-viewer-save-{nonce}").as_str())
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
        Self::marker(&root, &nonce)?;
        let mut names = std::fs::read_dir(&root)
            .map_err(|_| "cached root entries")?
            .take(3)
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "cached root entry")?;
        names.sort();
        if names
            != [
                std::ffi::OsString::from("downloads"),
                std::ffi::OsString::from("owner.json"),
            ]
        {
            return Err("fresh cached root contains unexpected entries".into());
        }
        if std::fs::read_dir(&downloads)
            .map_err(|_| "cached downloads")?
            .next()
            .is_some()
        {
            return Err("fresh empty cached downloads required".into());
        }
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
        Self::marker(&self.root, &self.nonce)?;
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
            2 | 3 => {
                unlinked(path, false)?;
                Ok("existing")
            }
            _ => Err("workflow destination on cancellation attempt".into()),
        }
    }
}
