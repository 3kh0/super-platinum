//! Upload names are metadata; local staging paths only identify the bytes.
use super::*;

pub struct UploadFile {
    pub path: PathBuf,
    pub filename: String,
    pub progress: Arc<AtomicU64>,
}

impl UploadFile {
    fn validate(&self) -> Result<(), Error> {
        // Preserve the existing native-path filename check. In particular, an
        // invalid UTF-8 chooser basename is not silently renamed to its UI fallback.
        if self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_none()
            || self.filename.is_empty()
            || self.filename.len() > 1024
            || self.filename.chars().any(|c| matches!(c, '\0' | '/'))
        {
            return Err(Error::Transport("attachment has no valid filename".into()));
        }
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct UploadUrl {
    upload_url: String,
    file_id: String,
}

pub(super) async fn stage_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    files_to_upload: Vec<UploadFile>,
    canceled: &AtomicBool,
) -> Result<Vec<serde_json::Value>, Error> {
    if canceled.load(Ordering::Relaxed) {
        return Err(Error::UploadCanceled);
    }
    // Reject the entire metadata set before staging any remote file. A malformed
    // later name must not strand an earlier file at the upload service.
    for file in &files_to_upload {
        file.validate()?;
    }
    let mut files = Vec::with_capacity(files_to_upload.len());
    for file in files_to_upload {
        if canceled.load(Ordering::Relaxed) {
            return Err(Error::UploadCanceled);
        }
        let UploadFile {
            path,
            filename,
            progress,
        } = file;
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| Error::Transport(format!("read {}: {error}", path.display())))?;
        let value = transport
            .execute(client.rest_form(
                workspace,
                "files.getUploadURLExternal",
                vec![
                    ("filename", filename.clone()),
                    ("length", bytes.len().to_string()),
                ],
            ))
            .await?;
        let ticket: UploadUrl = decode(value, "files.getUploadURLExternal")?;
        transport
            .upload_bytes(&ticket.upload_url, bytes, progress)
            .await?;
        if canceled.load(Ordering::Relaxed) {
            return Err(Error::UploadCanceled);
        }
        files.push(serde_json::json!({ "id": ticket.file_id, "title": filename }));
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(name: &str) -> UploadFile {
        UploadFile {
            path: "opaque.txt".into(),
            filename: name.into(),
            progress: Arc::new(AtomicU64::new(0)),
        }
    }

    #[test]
    fn names_are_metadata_and_keep_native_chooser_characters() {
        for name in [
            "日本語 🦊.png",
            "résumé & 100%.txt",
            "CON",
            "name\\with\ttab.txt",
        ] {
            assert!(file(name).validate().is_ok());
        }
        for name in ["", "path/name.txt", "bad\0name", &"a".repeat(1025)] {
            assert!(file(name).validate().is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn invalid_utf8_native_basename_keeps_existing_rejection() {
        use std::os::unix::ffi::OsStringExt;
        let mut input = file("attachment");
        input.path = std::ffi::OsString::from_vec(vec![b'a', 255]).into();
        assert!(input.validate().is_err());
    }
}
