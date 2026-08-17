use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::slack::models::{TeamId, UserId};

pub type AccountId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub d_cookie: String,
    pub workspaces: BTreeMap<TeamId, WorkspaceSession>,
}

impl Session {
    pub fn account_label(&self) -> String {
        let Some(workspace) = self.workspaces.values().next() else {
            return "Slack account".to_owned();
        };
        let more = self.workspaces.len().saturating_sub(1);
        if more == 0 {
            format!("{} · {}", workspace.name, workspace.user_id)
        } else {
            format!("{} +{more} · {}", workspace.name, workspace.user_id)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Accounts {
    pub active_account: AccountId,
    pub sessions: BTreeMap<AccountId, Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSession {
    pub team_id: TeamId,
    pub enterprise_id: Option<TeamId>,
    pub user_id: UserId,
    pub name: String,
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceMeta {
    team_id: TeamId,
    enterprise_id: Option<TeamId>,
    user_id: UserId,
    name: String,
    url: String,
}

impl From<&WorkspaceSession> for WorkspaceMeta {
    fn from(w: &WorkspaceSession) -> Self {
        Self {
            team_id: w.team_id.clone(),
            enterprise_id: w.enterprise_id.clone(),
            user_id: w.user_id.clone(),
            name: w.name.clone(),
            url: w.url.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionMeta {
    workspaces: Vec<WorkspaceMeta>,
}

impl From<&Session> for SessionMeta {
    fn from(session: &Session) -> Self {
        Self {
            workspaces: session
                .workspaces
                .values()
                .map(WorkspaceMeta::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountsMeta {
    active_account: AccountId,
    accounts: BTreeMap<AccountId, SessionMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StoredMeta {
    Accounts(AccountsMeta),
    Legacy(SessionMeta),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionSecrets {
    d_cookie: String,
    tokens: BTreeMap<TeamId, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountsSecrets {
    accounts: BTreeMap<AccountId, SessionSecrets>,
}

impl From<&Session> for SessionSecrets {
    fn from(session: &Session) -> Self {
        Self {
            d_cookie: session.d_cookie.clone(),
            tokens: session
                .workspaces
                .values()
                .map(|w| (w.team_id.clone(), w.token.clone()))
                .collect(),
        }
    }
}

mod appearance;
#[cfg(test)]
use appearance::settings_path;
pub use appearance::*;

static STORAGE_ROOT_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// On-disk identity for the config/data directories and the Keychain service.
///
/// These deliberately still say `snack` after the Super Platinum rebrand.
/// Changing either value orphans the existing session and warm cache, signing
/// every user out on first launch of the renamed build. Only rename them
/// alongside a migration that moves the old directory and Keychain item across.
#[cfg(not(test))]
const STORAGE_QUALIFIER: (&str, &str, &str) = ("com", "echonet", "snack");
#[cfg(all(not(test), not(debug_assertions)))]
const KEYRING_SERVICE: &str = "com.echonet.snack";

/// Isolate persistence for a downstream shell's test process.
///
/// Dependency crates are not compiled with `cfg(test)` when their consumers are
/// tested, so shell tests must opt into the same temporary storage boundary.
#[doc(hidden)]
pub fn use_test_storage() {
    let root = std::env::temp_dir().join(format!("super-platinum-test-{}", std::process::id()));
    let _ = STORAGE_ROOT_OVERRIDE.set(root);
}

pub fn config_dir() -> Result<PathBuf, AppError> {
    if let Some(root) = STORAGE_ROOT_OVERRIDE.get() {
        return Ok(root.join("config"));
    }

    // Exactly one of these blocks survives cfg expansion and becomes the tail
    // expression, so neither needs an explicit `return`.
    #[cfg(test)]
    {
        Ok(std::env::temp_dir().join(format!("super-platinum-test-{}", std::process::id())))
    }

    #[cfg(not(test))]
    {
        let dirs = directories::ProjectDirs::from(
            STORAGE_QUALIFIER.0,
            STORAGE_QUALIFIER.1,
            STORAGE_QUALIFIER.2,
        )
        .ok_or_else(|| AppError::Io(std::io::Error::other("no home directory for config path")))?;
        Ok(dirs.config_dir().to_path_buf())
    }
}

pub fn data_dir() -> Result<PathBuf, AppError> {
    if let Some(root) = STORAGE_ROOT_OVERRIDE.get() {
        return Ok(root.join("data"));
    }

    // Exactly one of these blocks survives cfg expansion and becomes the tail
    // expression, so neither needs an explicit `return`.
    #[cfg(test)]
    {
        Ok(std::env::temp_dir().join(format!("super-platinum-test-data-{}", std::process::id())))
    }

    #[cfg(not(test))]
    {
        let dirs = directories::ProjectDirs::from(
            STORAGE_QUALIFIER.0,
            STORAGE_QUALIFIER.1,
            STORAGE_QUALIFIER.2,
        )
        .ok_or_else(|| AppError::Io(std::io::Error::other("no home directory for data path")))?;
        Ok(dirs.data_local_dir().to_path_buf())
    }
}

fn session_path() -> Result<PathBuf, AppError> {
    Ok(config_dir()?.join("session.json"))
}

fn secret_account() -> &'static str {
    "session_v3"
}

fn legacy_secret_account() -> &'static str {
    "session_v2"
}

fn token_account(team_id: &str) -> String {
    format!("xoxc:{team_id}")
}

#[cfg(all(not(test), not(debug_assertions)))]
fn entry(account: &str) -> Result<keyring::Entry, AppError> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, account)?)
}

#[cfg(all(not(test), debug_assertions))]
fn dev_secret_path() -> Result<PathBuf, AppError> {
    Ok(config_dir()?.join("session.secrets.dev.json"))
}

#[cfg(all(not(test), debug_assertions))]
fn read_dev_secrets() -> Result<BTreeMap<String, String>, AppError> {
    match std::fs::read(dev_secret_path()?) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(all(not(test), debug_assertions))]
fn write_dev_secrets(secrets: &BTreeMap<String, String>) -> Result<(), AppError> {
    let path = dev_secret_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(secrets)?;
    std::fs::write(&path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(all(not(test), debug_assertions))]
fn set_secret(account: &str, value: &str) -> Result<(), AppError> {
    let mut secrets = read_dev_secrets()?;
    secrets.insert(account.to_owned(), value.to_owned());
    write_dev_secrets(&secrets)
}

#[cfg(all(not(test), not(debug_assertions)))]
fn set_secret(account: &str, value: &str) -> Result<(), AppError> {
    Ok(entry(account)?.set_password(value)?)
}

#[cfg(all(not(test), debug_assertions))]
fn get_secret(account: &str) -> Result<Option<String>, AppError> {
    Ok(read_dev_secrets()?.remove(account))
}

#[cfg(all(not(test), not(debug_assertions)))]
fn get_secret(account: &str) -> Result<Option<String>, AppError> {
    match entry(account)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(all(not(test), debug_assertions))]
fn delete_secret(account: &str) {
    if let Ok(mut secrets) = read_dev_secrets() {
        secrets.remove(account);
        let _ = write_dev_secrets(&secrets);
    }
}

#[cfg(all(not(test), not(debug_assertions)))]
fn delete_secret(account: &str) {
    if let Ok(e) = entry(account) {
        let _ = e.delete_credential();
    }
}

#[cfg(test)]
fn test_secrets() -> &'static Mutex<BTreeMap<String, String>> {
    static SECRETS: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
    SECRETS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(test)]
fn set_secret(account: &str, value: &str) -> Result<(), AppError> {
    test_secrets()
        .lock()
        .expect("test secret store poisoned")
        .insert(account.to_owned(), value.to_owned());
    Ok(())
}

#[cfg(test)]
fn get_secret(account: &str) -> Result<Option<String>, AppError> {
    Ok(test_secrets()
        .lock()
        .expect("test secret store poisoned")
        .get(account)
        .cloned())
}

#[cfg(test)]
fn delete_secret(account: &str) {
    let _ = test_secrets()
        .lock()
        .expect("test secret store poisoned")
        .remove(account);
}

fn load_legacy_secrets(meta: &SessionMeta) -> Result<Option<SessionSecrets>, AppError> {
    if let Some(json) = get_secret(legacy_secret_account())? {
        return Ok(Some(serde_json::from_str(&json)?));
    }

    let Some(d_cookie) = get_secret("d_cookie")? else {
        return Ok(None);
    };
    let mut tokens = BTreeMap::new();
    for w in &meta.workspaces {
        if let Some(token) = get_secret(&token_account(&w.team_id))? {
            tokens.insert(w.team_id.clone(), token);
        }
    }
    if tokens.is_empty() {
        return Ok(None);
    }
    Ok(Some(SessionSecrets { d_cookie, tokens }))
}

fn session_from_parts(meta: SessionMeta, secrets: &SessionSecrets) -> Option<Session> {
    let mut workspaces = BTreeMap::new();
    for w in meta.workspaces {
        let Some(token) = secrets.tokens.get(&w.team_id).cloned() else {
            continue;
        };
        workspaces.insert(
            w.team_id.clone(),
            WorkspaceSession {
                team_id: w.team_id,
                enterprise_id: w.enterprise_id,
                user_id: w.user_id,
                name: w.name,
                url: w.url,
                token,
            },
        );
    }
    (!workspaces.is_empty()).then(|| Session {
        d_cookie: secrets.d_cookie.clone(),
        workspaces,
    })
}

fn metadata(accounts: &Accounts) -> AccountsMeta {
    AccountsMeta {
        active_account: accounts.active_account.clone(),
        accounts: accounts
            .sessions
            .iter()
            .map(|(id, session)| (id.clone(), SessionMeta::from(session)))
            .collect(),
    }
}

fn write_metadata_value(meta: &AccountsMeta) -> Result<(), AppError> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(meta)?;
    let path = session_path()?;
    let temp = dir.join(format!(".session-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temp, json)?;
    #[cfg(target_os = "windows")]
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(temp);
            Err(error.into())
        }
    }
}

fn write_metadata(accounts: &Accounts) -> Result<(), AppError> {
    write_metadata_value(&metadata(accounts))
}

fn write_accounts(accounts: &Accounts) -> Result<(), AppError> {
    let secrets = AccountsSecrets {
        accounts: accounts
            .sessions
            .iter()
            .map(|(id, session)| (id.clone(), SessionSecrets::from(session)))
            .collect(),
    };
    set_secret(secret_account(), &serde_json::to_string(&secrets)?)?;
    write_metadata(accounts)
}

fn remove_legacy_secrets(meta: &SessionMeta) {
    for workspace in &meta.workspaces {
        delete_secret(&token_account(&workspace.team_id));
    }
    delete_secret("d_cookie");
    delete_secret(legacy_secret_account());
}

fn migrate_legacy(meta: SessionMeta) -> Result<Option<Accounts>, AppError> {
    let Some(secrets) = load_legacy_secrets(&meta)? else {
        return Ok(None);
    };
    let Some(session) = session_from_parts(meta.clone(), &secrets) else {
        return Ok(None);
    };
    let account_id = uuid::Uuid::new_v4().to_string();
    let accounts = Accounts {
        active_account: account_id.clone(),
        sessions: BTreeMap::from([(account_id, session)]),
    };
    write_accounts(&accounts)?;
    remove_legacy_secrets(&meta);
    Ok(Some(accounts))
}

pub fn load_accounts() -> Result<Option<Accounts>, AppError> {
    let path = session_path()?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let meta = match serde_json::from_slice::<StoredMeta>(&bytes)? {
        StoredMeta::Accounts(meta) => meta,
        StoredMeta::Legacy(meta) => return migrate_legacy(meta),
    };
    let Some(json) = get_secret(secret_account())? else {
        return Ok(None);
    };
    let secrets: AccountsSecrets = serde_json::from_str(&json)?;
    let sessions: BTreeMap<_, _> = meta
        .accounts
        .into_iter()
        .filter_map(|(id, session_meta)| {
            let session = session_from_parts(session_meta, secrets.accounts.get(&id)?)?;
            Some((id, session))
        })
        .collect();
    if sessions.is_empty() {
        return Ok(None);
    }
    let active_account = if sessions.contains_key(&meta.active_account) {
        meta.active_account
    } else {
        sessions
            .keys()
            .next()
            .cloned()
            .expect("sessions is not empty")
    };
    Ok(Some(Accounts {
        active_account,
        sessions,
    }))
}

pub fn load_session() -> Result<Option<Session>, AppError> {
    let Some(accounts) = load_accounts()? else {
        return Ok(None);
    };
    Ok(accounts.sessions.get(&accounts.active_account).cloned())
}

fn same_identity(a: &Session, b: &Session) -> bool {
    a.workspaces.values().any(|workspace| {
        b.workspaces
            .get(&workspace.team_id)
            .is_some_and(|other| other.user_id == workspace.user_id)
    })
}

/// Add or refresh a Slack account and make it active.
pub fn save_session(session: &Session) -> Result<(), AppError> {
    let saved = match load_accounts() {
        Ok(accounts) => accounts,
        Err(AppError::Json(error)) => {
            tracing::warn!(%error, "replacing corrupt saved session during sign-in");
            None
        }
        Err(error) => return Err(error),
    };
    let mut accounts = saved.unwrap_or_else(|| {
        let account_id = uuid::Uuid::new_v4().to_string();
        Accounts {
            active_account: account_id,
            sessions: BTreeMap::new(),
        }
    });
    let account_id = accounts
        .sessions
        .iter()
        .find_map(|(id, saved)| same_identity(saved, session).then(|| id.clone()))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    accounts.active_account = account_id.clone();
    accounts.sessions.insert(account_id, session.clone());
    write_accounts(&accounts)
}

pub fn set_active_account(account_id: &str) -> Result<(), AppError> {
    let bytes = match std::fs::read(session_path()?) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(
                std::io::Error::new(std::io::ErrorKind::NotFound, "no saved accounts").into(),
            );
        }
        Err(e) => return Err(e.into()),
    };
    let mut meta = match serde_json::from_slice::<StoredMeta>(&bytes)? {
        StoredMeta::Accounts(meta) => meta,
        StoredMeta::Legacy(_) => {
            let Some(accounts) = load_accounts()? else {
                return Err(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "no saved accounts").into(),
                );
            };
            metadata(&accounts)
        }
    };
    if !meta.accounts.contains_key(account_id) {
        return Err(
            std::io::Error::new(std::io::ErrorKind::NotFound, "saved account not found").into(),
        );
    }
    meta.active_account = account_id.to_owned();
    write_metadata_value(&meta)
}

pub fn remove_account(account_id: &str) -> Result<Option<AccountId>, AppError> {
    let Some(mut accounts) = load_accounts()? else {
        clear_session()?;
        return Ok(None);
    };
    accounts.sessions.remove(account_id);
    if accounts.sessions.is_empty() {
        clear_session()?;
        return Ok(None);
    }
    if accounts.active_account == account_id {
        accounts.active_account = accounts
            .sessions
            .keys()
            .next()
            .cloned()
            .expect("sessions is not empty");
    }
    write_accounts(&accounts)?;
    Ok(Some(accounts.active_account))
}

pub fn clear_session() -> Result<(), AppError> {
    if let Ok(meta_bytes) = std::fs::read(session_path()?)
        && let Ok(StoredMeta::Legacy(meta)) = serde_json::from_slice::<StoredMeta>(&meta_bytes)
    {
        for workspace in &meta.workspaces {
            delete_secret(&token_account(&workspace.team_id));
        }
    }
    delete_secret("d_cookie");
    delete_secret(legacy_secret_account());
    delete_secret(secret_account());
    match std::fs::remove_file(session_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests;
