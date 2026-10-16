//! Existing owned-directory/ordinal capture ACK protocol, with private Save sentinel.
use super::*;
use std::path::{Path, PathBuf};
use super_platinum_core::file_download::{DownloadError, SaveMode, StagedFile};
pub(super) const SENTINEL: &[u8] = b"refresh-save-sentinel\n";
#[derive(Clone)]
pub(super) struct Harness {
    pub root: PathBuf,
    pub identity: Value,
}
pub(super) fn configuration() -> Result<Harness, String> {
    if !enabled() {
        return Err("Refresh fixture opt-in required".into());
    }
    let nonce = std::env::var("SUPER_PLATINUM_FILE_REFRESH_NONCE").map_err(|_| "Missing nonce")?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let root = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_FILE_REFRESH_ROOT").ok_or("Missing owned root")?,
    );
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !root.is_absolute()
        || root.file_name().and_then(|s| s.to_str())
            != Some(format!("file-refresh-{nonce}").as_str())
    {
        return Err("Invalid owned refresh address".into());
    }
    Ok(Harness {
        root,
        identity: json!({"version":1,"nonce":nonce,"fixture":alias,"pid":std::process::id()}),
    })
}
fn no_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
}
fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Missing owned file")?;
    if !meta.is_file() || !no_link(&meta) || meta.len() > max {
        return Err("Invalid owned file".into());
    }
    std::fs::read(path).map_err(|_| "Owned read failed".into())
}
impl Harness {
    fn validate(&self) -> Result<(), String> {
        for path in self.root.ancestors() {
            let meta = std::fs::symlink_metadata(path).map_err(|_| "Missing owned ancestor")?;
            if !meta.is_dir() || !no_link(&meta) {
                return Err("Redirected owned ancestor".into());
            }
        }
        let owner: Value = super_platinum_core::slash_commands::codec::decode(
            &read_regular(&self.root.join("owner.json"), 1024)?,
            1024,
        )
        .map_err(|_| "Invalid owner JSON")?;
        if owner
            != json!({"version":1,"nonce":self.identity["nonce"],"fixture":self.identity["fixture"]})
        {
            return Err("Wrong refresh owner".into());
        }
        Ok(())
    }
    pub async fn stage_download(&self, bytes: Arc<[u8]>) -> Result<StagedFile, String> {
        let h = self.clone();
        let path = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            use std::io::Write;
            h.validate()?;
            let entries = std::fs::read_dir(&h.root)
                .map_err(|_| "Owned directory unreadable")?
                .map(|e| e.map(|e| e.file_name()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Owned entries unreadable")?;
            if entries.len() != 1 || entries[0] != "owner.json" {
                return Err("Refresh root must be fresh".into());
            }
            let path = h.root.join("sentinel.txt");
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(|_| "Exclusive sentinel refused")?;
            f.write_all(SENTINEL).map_err(|_| "Sentinel write failed")?;
            f.sync_all().map_err(|_| "Sentinel sync failed")?;
            Ok(path)
        })
        .await
        .map_err(|_| "Sentinel task failed")??;
        let mut stage = StagedFile::create(path, SaveMode::ConfirmedCachedOverwrite)
            .await
            .map_err(|_| "Stage creation failed")?;
        stage
            .cached(&bytes, || true)
            .await
            .map_err(|_| "Old bytes staging failed")?;
        Ok(stage)
    }
    pub async fn capture(&self, ordinal: usize, receipt: Value) -> Result<(), String> {
        let h = self.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            h.validate()?;
            match std::fs::symlink_metadata(h.root.join(format!("{ordinal}.ack"))) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                _ => Err("Reused ACK refused".into()),
            }
        })
        .await
        .map_err(|_| "ACK preflight task failed")??;
        emit("file-refresh-stage", receipt).await?;
        for _ in 0..600 {
            let h = self.clone();
            let found = tokio::task::spawn_blocking(move || -> Result<bool, String> {
                h.validate()?;
                let path = h.root.join(format!("{ordinal}.ack"));
                match std::fs::symlink_metadata(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                    Err(_) => Err("ACK inspection failed".into()),
                    Ok(_) => {
                        if read_regular(&path, 64)? == b"captured\n" {
                            Ok(true)
                        } else {
                            Err("Malformed ACK".into())
                        }
                    }
                }
            })
            .await
            .map_err(|_| "ACK read task failed")??;
            if found {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err("Refresh capture ACK timeout".into())
    }
    pub async fn canceled(
        &self,
        stage: StagedFile,
        cancel: Arc<AtomicBool>,
    ) -> Result<Value, String> {
        let latched = cancel.load(Ordering::Acquire);
        let canceled = matches!(
            stage.commit(|| true, cancel).await,
            Err(DownloadError::Canceled)
        );
        if !latched || !canceled {
            return Err("Old staged download was not canceled".into());
        }
        // The real StagedFile drop owns its asynchronous part cleanup.
        for _ in 0..150 {
            let h = self.clone();
            let clean = tokio::task::spawn_blocking(move || -> Result<bool, String> {
                h.validate()?;
                if read_regular(&h.root.join("sentinel.txt"), 1024)? != SENTINEL {
                    return Err("Sentinel replaced".into());
                }
                for entry in std::fs::read_dir(&h.root).map_err(|_| "Owned entries unreadable")? {
                    let entry = entry.map_err(|_| "Owned entry unreadable")?;
                    if entry.file_name().to_string_lossy().ends_with(".part") {
                        return Ok(false);
                    }
                }
                Ok(true)
            })
            .await
            .map_err(|_| "Save cleanup observation failed")??;
            if clean {
                return Ok(
                    json!({"latched":true,"commit":"Canceled","sentinel":"refresh-save-sentinel\n","partFiles":0,"rendererCurrentPassed":true}),
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("Old staging cleanup did not settle".into())
    }
}
pub(super) async fn emit(prefix: &'static str, value: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        std::io::stderr()
            .lock()
            .write_all(format!("{prefix}: {value}\n").as_bytes())
    })
    .await
    .map_err(|_| "Receipt writer failed")?
    .map_err(|_| "Receipt output failed".into())
}
