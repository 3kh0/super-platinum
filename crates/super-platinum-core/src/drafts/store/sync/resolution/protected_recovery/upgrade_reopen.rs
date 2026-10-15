//! Fixture-only disk identity locator. Decoding it never creates a live permit.
use super::*;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Ticket {
    version: u8,
    nonce: String,
    account: String,
    marker: Vec<u64>,
    database: Vec<u64>,
}

impl Owned {
    pub(in crate::drafts::store) fn fixture_ticket(&self, file: &File) -> Result<String, AppError> {
        self.check()?;
        let nonce = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("super-platinum-upgrade-"))
            .ok_or_else(|| invalid("not an owned fixture root"))?;
        let database = identity(file)?;
        if self.retain_db()?.as_ref().map(|(_, id)| id) != Some(&database) {
            return Err(invalid("fixture export file changed"));
        }
        Ok(serde_json::to_string(&Ticket {
            version: 1,
            nonce: nonce.to_owned(),
            account: self.account.clone(),
            marker: self.identity.0.clone(),
            database: database.0,
        })?)
    }

    pub(in crate::drafts::store) fn reopen_fixture(encoded: &str) -> Result<Arc<Self>, AppError> {
        if encoded.len() > 1024 {
            return Err(invalid("fixture locator too large"));
        }
        let ticket: Ticket = serde_json::from_str(encoded)?;
        if ticket.version != 1
            || !valid_uuid(&ticket.account)
            || !valid_uuid(&ticket.nonce)
            || ticket.marker.len() != 2
            || ticket.database.len() != 2
            || uuid::Uuid::parse_str(&ticket.account)
                .ok()
                .map(|v| v.to_string())
                .as_deref()
                != Some(&ticket.account)
            || uuid::Uuid::parse_str(&ticket.nonce)
                .ok()
                .map(|v| v.to_string())
                .as_deref()
                != Some(&ticket.nonce)
        {
            return Err(invalid("invalid fixture locator"));
        }
        // No path is accepted from the caller. Only the exact previously-created
        // nonce directory immediately beneath this process's canonical temp root.
        let base = std::fs::canonicalize(std::env::temp_dir())?;
        plain_chain(&base)?;
        let root = base.join(format!("super-platinum-upgrade-{}", ticket.nonce));
        plain_chain(&root)?;
        if !std::fs::symlink_metadata(&root)?.is_dir() {
            return Err(invalid("fixture root is not a directory"));
        }
        let marker_path = root.join("owner");
        plain_chain(&marker_path)?;
        if !std::fs::symlink_metadata(&marker_path)?.is_file() {
            return Err(invalid("fixture marker is not a file"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let marker = options.open(marker_path)?;
        marker
            .try_lock()
            .map_err(|_| invalid("fixture account still owned"))?;
        let marker_identity = identity(&marker)?;
        if marker_identity.0 != ticket.marker {
            return Err(invalid("fixture marker identity changed"));
        }
        let path = root.join(format!("drafts-{}.sqlite", ticket.account));
        plain_chain(&path)?;
        if !std::fs::symlink_metadata(&path)?.is_file() {
            return Err(invalid("fixture database is not a file"));
        }
        let file = retained(&path)?;
        let database_identity = identity(&file)?;
        if database_identity.0 != ticket.database {
            return Err(invalid("fixture database identity changed"));
        }
        let owner = Arc::new(Self {
            root,
            path,
            account: ticket.account,
            marker: Mutex::new(marker),
            identity: marker_identity,
            worker: Mutex::new(None),
            reopened: Some((file, database_identity)),
        });
        owner.check()?;
        owner.readonly_preflight(&owner.reopened.as_ref().unwrap().0)?;
        Ok(owner)
    }
}
