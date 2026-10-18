//! Closed configured path constructor used only by the account coordinator.
//! Never accepts a database filename, SQL, or readiness/drain assertion.
use super::*;

impl Owned {
    pub(in crate::drafts::store) fn configured(
        account: &str,
        root: PathBuf,
    ) -> Result<Arc<Self>, AppError> {
        if uuid::Uuid::parse_str(account)
            .ok()
            .map(|id| id.to_string())
            .as_deref()
            != Some(account)
            || !root.is_absolute()
            || root
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(invalid("invalid configured account directory"));
        }
        // Reject existing redirecting ancestors before creating missing dirs.
        for ancestor in root.ancestors() {
            match std::fs::symlink_metadata(ancestor) {
                Ok(_) => {
                    plain_chain(ancestor)?;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        std::fs::create_dir_all(&root)?;
        plain_chain(&root)?;
        let marker_path = root.join(format!("drafts-{account}.owner"));
        let mut options = OpenOptions::new();
        options.write(true).read(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let (mut marker, created) = match options.open(&marker_path) {
            Ok(file) => (file, true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                plain_chain(&marker_path)?;
                if !std::fs::symlink_metadata(&marker_path)?.is_file() {
                    return Err(invalid("configured marker is not a file"));
                }
                options.create_new(false);
                (options.open(&marker_path)?, false)
            }
            Err(e) => return Err(e.into()),
        };
        marker
            .try_lock()
            .map_err(|_| invalid("configured account already owned"))?;
        if created {
            marker.write_all(account.as_bytes())?;
            marker.sync_all()?;
        }
        let identity = identity(&marker)?;
        let path = root.join(format!("drafts-{account}.sqlite"));
        let mut owner = Self {
            root,
            marker_path,
            path,
            account: account.into(),
            marker: Mutex::new(marker),
            identity,
            worker: Mutex::new(None),
            reopened: None,
        };
        owner.check()?;
        owner.reopened = owner.retain_db()?;
        Ok(Arc::new(owner))
    }
}
