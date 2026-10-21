//! Canonical display facts authorize one owned click, never a preview URL.
//! A complete files.info target and final durable reply capture are required.
use super::*;
use file_source::Token;
use serde_json::Value;
use super_platinum_core::slack::models::File;

struct Request {
    model: Signal<State>,
    id: String,
}
impl Request {
    fn current(&self, value: &State) -> bool {
        value.pending_file.as_deref() == Some(&self.id)
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Ok(mut value) = self.model.try_write() {
            if value.pending_file.as_deref() == Some(&self.id) {
                value.pending_file = None;
            }
        }
    }
}

pub(super) fn open(state: Signal<ShellState>, mut model: Signal<State>, token: Token) {
    {
        let Ok(shell) = state.try_read() else { return };
        let Ok(value) = model.try_read() else { return };
        if !token.current(&shell, &value) || value.pending_file.is_some() {
            return;
        }
    }
    let request = Request {
        model,
        id: uuid::Uuid::new_v4().to_string(),
    };
    {
        let mut value = model.write();
        value.pending_file = Some(request.id.clone());
        if value.mutation_error
            == Some("This file could not be opened. Your reply draft is retained here.")
        {
            value.mutation_error = None;
        }
    }
    dioxus::core::spawn_forever(async move {
        let current = |shell: &ShellState, value: &State| {
            request.current(value) && token.current(shell, value)
        };
        let result = resolve(state, &token).await;
        let active = state
            .try_read()
            .ok()
            .zip(model.try_read().ok())
            .is_some_and(|(shell, value)| current(&shell, &value));
        if !active {
            return;
        }
        let Ok(file) = result else {
            refused(model);
            return;
        };
        let Ok(departure) = reply::prepare_departure(state, model, &current).await else {
            // Retirement during capture belongs to the original entry. Do not
            // attach its navigation error to a successor thread.
            if state
                .try_read()
                .ok()
                .zip(model.try_read().ok())
                .is_some_and(|(shell, value)| current(&shell, &value))
            {
                refused(model);
            }
            return;
        };
        if !departure.current()
            || !state
                .try_read()
                .ok()
                .zip(model.try_read().ok())
                .is_some_and(|(shell, value)| current(&shell, &value))
        {
            return;
        }
        // No await or unlocked editor frame between the final owner checks and
        // the opener. A failed opener leaves the original retained draft editable.
        let before = state.peek().shortcut_generation;
        let previous_list = crate::slack_lists::generation(&state.peek());
        let canvas = matches!(file.filetype.as_deref(), Some("quip" | "canvas"));
        let list = super_platinum_core::slack_lists::is_list(&file);
        crate::files::open(state, file);
        let committed = {
            let shell = state.peek();
            if canvas {
                shell.overlay == Some(crate::state::Overlay::Canvas)
                    && shell.shortcut_generation != before
                    && shell.canvas.as_ref().is_some_and(|c| c.id == token.id())
            } else if list {
                shell.overlay == Some(crate::state::Overlay::SlackList)
                    && crate::slack_lists::presentation(&shell).is_some_and(|v| {
                        v.list_id == token.id() && Some(v.generation) != previous_list
                    })
            } else {
                shell.overlay == Some(crate::state::Overlay::Viewer)
                    && shell
                        .viewer
                        .as_ref()
                        .is_some_and(|v| v.file.as_ref().is_some_and(|f| f.id == token.id()))
            }
        };
        if committed {
            departure.committed();
        } else {
            refused(model);
        }
    });
}

fn refused(mut model: Signal<State>) {
    if let Ok(mut value) = model.try_write() {
        value.mutation_error =
            Some("This file could not be opened. Your reply draft is retained here.");
    }
}

async fn resolve(state: Signal<ShellState>, token: &Token) -> Result<File, Error> {
    let response = if crate::fixture::is_fixture() {
        fixture::canonical_files::resolve(state, token.id()).await?
    } else {
        let context = token
            .context(&state.peek())
            .ok_or_else(|| Error::Api("canvas_file_source_retired".into()))?;
        context
            .transport
            .execute(api::file_info(
                &context.client,
                &context.session,
                token.id(),
            ))
            .await?
    };
    target(&response, token.id())
}

fn target(response: &Value, id: &str) -> Result<File, Error> {
    let invalid = || Error::Api("canvas_comment_file_unavailable".into());
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(invalid());
    }
    let mut raw = response.get("file").cloned().ok_or_else(invalid)?;
    if raw.get("id").and_then(Value::as_str) != Some(id) {
        return Err(invalid());
    }
    // The ordinary successful files.info producer explicitly clears is_deleted.
    // All other visibility facts still come from this fresh complete response.
    raw.as_object_mut()
        .ok_or_else(invalid)?
        .insert("is_deleted".into(), Value::Bool(false));
    if !super_platinum_core::canvas::comments::file_facts::should_show_file(&raw) {
        return Err(invalid());
    }
    serde_json::from_value(raw).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fresh_target_requires_success_identity_and_visibility() {
        let id = "F12345678";
        for response in [
            json!({"ok":false,"file":{"id":id}}),
            json!({"ok":true,"file":{"id":"F87654321"}}),
            json!({"ok":true,"file":{"id":id,"file_access":"access_denied"}}),
            json!({"ok":true,"file":{"id":id,"is_tombstoned":true}}),
            json!({"ok":true,"file":{"id":id,"restriction_type":1}}),
        ] {
            assert!(target(&response, id).is_err());
        }
        let raw = json!({"ok":true,"file":{"id":id,"mode":"hosted","name":"a.pdf",
            "title":"Preview title","is_deleted":true,"mimetype":"application/pdf",
            "url_private":"https://files.slack.com/a.pdf","opaque": {"kept":true}}});
        let file = target(&raw, id).unwrap();
        assert_eq!(file.id.as_deref(), Some(id));
        assert_eq!(file.extra.get("opaque"), Some(&json!({"kept":true})));
        assert_eq!(raw["file"]["is_deleted"], true);
    }
}
