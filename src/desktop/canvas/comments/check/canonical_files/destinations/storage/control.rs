//! Two owned external SQLite lock acknowledgments; all filesystem work off UI.
use super::*;
use std::path::{Path, PathBuf};

fn regular(path: &Path) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Storage path unavailable")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Storage path is a reparse point".into());
        }
    }
    if m.file_type().is_symlink() {
        return Err("Storage path is a link".into());
    }
    Ok(m)
}
fn owner(directory: &Path, expected: &Value) -> Result<(), String> {
    if !regular(directory)?.is_dir() {
        return Err("Storage directory is not a directory".into());
    }
    let path = directory.join("owner.json");
    let m = regular(&path)?;
    if !m.is_file() || m.len() > 4096 {
        return Err("Storage owner file invalid".into());
    }
    let observed: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Storage owner read failed")?)
            .map_err(|_| "Storage owner JSON invalid")?;
    if &observed != expected {
        return Err("Storage owner replaced".into());
    }
    Ok(())
}
pub(super) struct Control {
    directory: PathBuf,
    owner: Value,
    fixture: String,
    nonce: String,
}
impl Control {
    pub(super) async fn capture(
        state: Signal<ShellState>,
        witness: &reply::fixture::DestinationWitness,
    ) -> Result<Self, String> {
        if !super::super::storage_requested() || !data::enabled(&state.peek()) {
            return Err("Storage case not enabled".into());
        }
        let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or("Storage ACK directory absent")?;
        if witness.private_path(state)? != directory.join("replies.sqlite") {
            return Err("Storage journal is not original private database".into());
        }
        let nonce = std::env::var("SUPER_PLATINUM_CANVAS_STORAGE_NONCE")
            .map_err(|_| "Storage nonce absent")?;
        if nonce.is_empty() || nonce.len() > 128 || !nonce.is_ascii() {
            return Err("Storage nonce invalid".into());
        }
        let fixture =
            std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Storage fixture absent")?;
        let expected = json!({"version":1,"nonce":nonce,"fixture":fixture});
        let path = directory.clone();
        let check = expected.clone();
        tokio::task::spawn_blocking(move || owner(&path, &check))
            .await
            .map_err(|_| "Storage owner task failed")??;
        Ok(Self {
            directory,
            owner: expected,
            fixture,
            nonce,
        })
    }
    pub(super) async fn command(
        &self,
        state: Signal<ShellState>,
        witness: &reply::fixture::DestinationWitness,
        sequence: u64,
    ) -> Result<Value, String> {
        if !(1..=2).contains(&sequence)
            || witness.private_path(state)? != self.directory.join("replies.sqlite")
        {
            return Err("Storage control original witness missing".into());
        }
        let action = if sequence == 1 { "hold" } else { "release" };
        let request = json!({"version":1,"nonce":self.nonce,"fixture":self.fixture,
            "pid":std::process::id(),"sequence":sequence,"action":action});
        let root = self.directory.clone();
        let expected = self.owner.clone();
        let marker = root.join(format!("storage-{sequence}.json"));
        let path = marker.clone();
        tokio::task::spawn_blocking(move || {
            owner(&root, &expected)?;
            match std::fs::symlink_metadata(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                _ => Err("Storage acknowledgment predates request".to_string()),
            }
        })
        .await
        .map_err(|_| "Storage marker task failed")??;
        let output = request.clone();
        tokio::task::spawn_blocking(move || {
            eprintln!("canvas-canonical-native-storage-control: {output}")
        })
        .await
        .map_err(|_| "Storage command output failed")?;
        for _ in 0..400 {
            let root = self.directory.clone();
            let expected = self.owner.clone();
            let path = marker.clone();
            let ack = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
                owner(&root, &expected)?;
                match std::fs::symlink_metadata(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(_) => return Err("Storage ACK unavailable".into()),
                    Ok(_) => (),
                }
                let m = regular(&path)?;
                if !m.is_file() || m.len() > 1024 * 1024 {
                    return Err("Storage ACK invalid".into());
                }
                serde_json::from_slice(&std::fs::read(path).map_err(|_| "Storage ACK read failed")?)
                    .map(Some)
                    .map_err(|_| "Storage ACK JSON invalid".into())
            })
            .await
            .map_err(|_| "Storage ACK task failed")??;
            if let Some(ack) = ack {
                if ["version", "nonce", "fixture", "pid", "sequence", "action"]
                    .iter()
                    .any(|k| ack[*k] != request[*k])
                    || ack["ok"] != true
                    || ack["account"] != "147b5480-9f1a-4443-89da-836472cbb209"
                    || ack["scope"] != json!({"workspace":"T1","user":"U0","file":"Ffixture"})
                    || ack["root"] != data::ROOT
                    || ack["journal"]["drafts"].as_array().map(Vec::len) != Some(1)
                    || !ack["journal"]["attempt"].is_null()
                    || ack["journal"]["history"] != json!([])
                    || ack["contentSha256"].as_str().is_none_or(|s| s.len() != 64)
                    || (sequence == 2 && ack["unchanged"] != true)
                {
                    return Err(format!("Storage ACK identity/baseline differs: {ack}"));
                }
                witness.runtime(state)?;
                return Ok(ack);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Err(format!("Storage {action} ACK timed out"))
    }
}
