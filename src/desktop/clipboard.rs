use base64::Engine;
use dioxus::prelude::{Signal, WritableExt};

use crate::state::ShellState;

/// Audited DOM bridge for clipboard file payloads. Browser file objects never
/// receive Slack credentials; bytes move directly from the WebView event to a
/// renderer-owned local file and then enter the normal native upload path.
pub async fn watch(mut state: Signal<ShellState>) {
    let mut bridge = dioxus::document::eval(
        r#"document.addEventListener('paste', async event => {
             const files = [...(event.clipboardData?.files ?? [])];
             if (!files.length) return;
             event.preventDefault();
             const payload = [];
             for (const file of files) {
               const bytes = new Uint8Array(await file.arrayBuffer());
               let binary = '';
               for (let offset = 0; offset < bytes.length; offset += 0x8000) {
                 binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
               }
               payload.push({name: file.name || 'pasted-file', mime: file.type, data: btoa(binary)});
             }
             dioxus.send(payload);
           });"#,
    );
    while let Ok(payload) = bridge.recv::<serde_json::Value>().await {
        let Some(files) = payload.as_array() else {
            continue;
        };
        let mut paths = Vec::new();
        for file in files {
            let name = file
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("pasted-file");
            let safe_name = name
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                        character
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            let Some(encoded) = file.get("data").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
                continue;
            };
            let Ok(root) = super_platinum_core::config::data_dir() else {
                continue;
            };
            let directory = root.join("pasted-attachments");
            if std::fs::create_dir_all(&directory).is_err() {
                continue;
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = directory.join(format!("{stamp}-{safe_name}"));
            if std::fs::write(&path, bytes).is_ok() {
                paths.push(path);
            }
        }
        if !paths.is_empty() {
            state.write().add_attachments(paths);
        }
    }
}
