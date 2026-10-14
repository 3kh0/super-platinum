use base64::Engine;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) mod file_check;
mod name;

#[cfg(test)]
mod tests;

use crate::state::{ComposerTarget, ShellState};

/// Audited DOM bridge for clipboard file payloads. Browser file objects never
/// receive Slack credentials; bytes move directly from the WebView event to a
/// renderer-owned local file and then enter the normal native upload path.
pub async fn watch(mut state: Signal<ShellState>) {
    let mut bridge = dioxus::document::eval(
        r#"document.addEventListener('paste', async event => {
             const files = [...(event.clipboardData?.files ?? [])];
             if (!files.length) return;
             const field = event.target;
             if (field?.tagName !== 'TEXTAREA' ||
                 !['channel-composer', 'thread-composer'].includes(field.id) ||
                 document.getElementById(field.id) !== field) return;
             const scope = field.getAttribute('data-draft-scope');
             if (!scope) return;
             event.preventDefault();
             const thread = field.id === 'thread-composer';
             const payload = [];
             for (const file of files) {
               const bytes = new Uint8Array(await file.arrayBuffer());
               let binary = '';
               for (let offset = 0; offset < bytes.length; offset += 0x8000) {
                 binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
               }
               payload.push({name: file.name || 'pasted-file', mime: file.type, data: btoa(binary)});
             }
             dioxus.send({thread, scope, files: payload});
           });"#,
    );
    while let Ok(payload) = bridge.recv::<serde_json::Value>().await {
        let Some(files) = payload.get("files").and_then(serde_json::Value::as_array) else {
            continue;
        };
        // A paste lands in the composer the reader is typing into.
        let target = if payload.get("thread").and_then(serde_json::Value::as_bool) == Some(true) {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        };
        let Some(scope) = payload.get("scope").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let fixture = crate::fixture::is_fixture();
        let storage = {
            let shell = state.peek();
            if !owns_destination(&shell, target, scope, fixture) {
                continue;
            }
            storage_mode(
                fixture,
                shell.core.session.is_none() && shell.core.transport.is_none(),
                std::env::var_os("SUPER_PLATINUM_FIXTURE_PASTE_ROOT").map(PathBuf::from),
            )
        };
        let Some(storage) = storage else { continue };
        let files = files.clone();
        // Decode and write away from the renderer. Keep no Signal borrow across
        // this await; a slow file must never land in a replacement conversation.
        let Ok(files) = tokio::task::spawn_blocking(move || save_files(storage, files)).await
        else {
            continue;
        };
        if files.is_empty() {
            continue;
        }
        if owns_destination(&state.peek(), target, scope, fixture) {
            state.write().add_named_attachments(
                files.into_iter().map(|file| (file.path, file.name)),
                target,
            );
        } else {
            // Only paths created by this operation are eligible for cleanup.
            let _ = tokio::task::spawn_blocking(move || {
                for file in files {
                    let _ = std::fs::remove_file(file.path);
                }
            })
            .await;
        }
    }
}

fn owns_destination(
    shell: &ShellState,
    target: ComposerTarget,
    scope: &str,
    fixture: bool,
) -> bool {
    shell.composer_key(target).is_some()
        && crate::view::composer::field_scope(shell, target) == scope
        && (!fixture || (shell.core.session.is_none() && shell.core.transport.is_none()))
}

#[derive(Debug, PartialEq, Eq)]
enum Storage {
    Production,
    Fixture(PathBuf),
}

/// The runner must create and own this private directory. Never infer it from
/// TMP, and never fall back to the user's config directory in a fixture.
fn storage_mode(fixture: bool, disconnected: bool, root: Option<PathBuf>) -> Option<Storage> {
    if fixture {
        let root = root?;
        (disconnected
            && root.is_absolute()
            && !root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)))
        .then_some(Storage::Fixture(root))
    } else if root.is_none() {
        Some(Storage::Production)
    } else {
        // A fixture override accidentally inherited by a real launch is inert.
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SavedFile {
    path: PathBuf,
    name: String,
}

fn save_files(storage: Storage, files: Vec<serde_json::Value>) -> Vec<SavedFile> {
    // Fixture payloads are deliberately bounded. Production upload limits and
    // behavior stay with the existing upload path.
    const FIXTURE_MAX_BYTES: usize = 16 * 1024 * 1024;
    let fixture = matches!(storage, Storage::Fixture(_));
    if fixture
        && (files.len() > 16
            || files
                .iter()
                .filter_map(|file| file.get("data").and_then(serde_json::Value::as_str))
                .map(str::len)
                .sum::<usize>()
                > FIXTURE_MAX_BYTES * 4 / 3 + 4)
    {
        return Vec::new();
    }
    let directory = match storage {
        Storage::Fixture(root) => {
            // Require a pre-existing ordinary directory; the runner, not the
            // application, owns its creation and eventual removal.
            let Ok(metadata) = std::fs::symlink_metadata(&root) else {
                return Vec::new();
            };
            if !ordinary_directory(&metadata) {
                return Vec::new();
            }
            let Ok(root) = root.canonicalize() else {
                return Vec::new();
            };
            root
        }
        Storage::Production => {
            let Ok(root) = super_platinum_core::config::data_dir() else {
                return Vec::new();
            };
            let directory = root.join("pasted-attachments");
            if std::fs::create_dir_all(&directory).is_err() {
                return Vec::new();
            }
            directory
        }
    };
    let mut paths = Vec::new();
    for file in files {
        let Some(encoded) = file.get("data").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            continue;
        };
        let Some(name) = name::display_name(file.get("name").and_then(serde_json::Value::as_str))
        else {
            continue;
        };
        // User-visible metadata never participates in filesystem addressing.
        // Keep only a bounded ASCII extension for existing preview/type checks.
        let disk_name = name::disk_name(&name);
        if let Some(path) = write_unique(&directory, &disk_name, &bytes) {
            paths.push(SavedFile { path, name });
        }
    }
    paths
}

fn write_unique(directory: &Path, name: &str, bytes: &[u8]) -> Option<PathBuf> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    for _ in 0..16 {
        let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = directory.join(format!("{stamp}-{}-{next}-{name}", std::process::id()));
        match write_new(&path, bytes) {
            Ok(()) => return Some(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn ordinary_directory(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Junctions and other reparse points can redirect the private root too.
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
