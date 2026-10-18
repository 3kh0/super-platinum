//! Profile-owned title observations. This new nonpersistent cache starts empty;
//! it is neither the typed file browser nor Slack's complete file collection.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::slack::{
    PreparedRequest,
    api::custom::texty::{file_title_source as source, file_titles},
};
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(in crate::profile_edit::sections) struct Cache {
    token: Token,
    store: source::Store,
    reads: BTreeMap<String, (u64, u64)>,
    changes: BTreeMap<String, Arc<()>>,
}

#[derive(Clone)]
pub(super) struct Observation {
    token: Token,
    id: String,
    row: source::Observation,
    quip: Option<crate::canvas::quip_titles::Observation>,
    socket: u64,
    revision: u64,
}
fn socket(shell: &ShellState, token: &Token) -> Option<u64> {
    token
        .current(shell)
        .then(|| {
            shell
                .core
                .workspaces
                .get(&token.owner.team)
                .map(|w| w.rt_generation)
        })
        .flatten()
}
impl Observation {
    pub(super) fn capture(shell: &ShellState, token: &Token, id: &str) -> Result<Self, Error> {
        let cache = shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .ok_or_else(unavailable)?;
        let (generation, revision) = *cache.reads.get(id).ok_or_else(unavailable)?;
        let row = cache.store.row(id).map_err(|_| unavailable())?;
        // File metadata does not establish the independent editor-title state.
        // Only block-title Canvas labels consume that runtime observation.
        let quip = if has_canvas_blocks(row.effective()) {
            Some(
                crate::canvas::quip_titles::Observation::capture(shell, id)
                    .ok_or_else(unavailable)?,
            )
        } else {
            None
        };
        let observed = Self {
            token: token.clone(),
            id: id.into(),
            row,
            quip,
            socket: generation,
            revision,
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        socket(shell, &self.token) == Some(self.socket)
            && self.quip.as_ref().is_none_or(|quip| quip.current(shell))
            && !crate::file_events::changed_since(
                shell,
                &self.token.owner.team,
                &self.id,
                self.revision,
            )
            && shell
                .core
                .workspaces
                .get(&self.token.owner.team)
                .is_some_and(|w| !w.deleted_files.contains(&self.id))
            && shell
                .profile_edit
                .sections
                .file_titles
                .as_ref()
                .is_some_and(|cache| {
                    cache.token == self.token
                        && cache.reads.get(&self.id) == Some(&(self.socket, self.revision))
                        && self.row.current(&cache.store)
                })
    }
    pub(super) fn label(&self, raw: &Value) -> Result<String, Error> {
        if raw["file_id"].as_str() != Some(self.id.as_str()) {
            return Err(unavailable());
        }
        let effective = self.row.effective();
        let canvas = effective["mode"] == "quip" || effective["filetype"] == "quip";
        let list = (effective["mode"] == "list" || effective["filetype"] == "list")
            && effective["subtype"] != "poll";
        let kind = match raw["type"].as_str() {
            Some("canvas") if canvas => file_titles::Kind::Canvas,
            Some("file") if list => file_titles::Kind::WholeList,
            _ => return Err(unavailable()),
        };
        let label = file_titles::label_with_quip(
            kind,
            file_titles::Status::Normal,
            effective,
            self.quip
                .as_ref()
                .map_or(file_titles::QuipTitle::Unavailable, |quip| quip.value()),
        )
        .map_err(|_| unavailable())?;
        // The same closed whole-token grammar is used for opening and capture.
        let mut context =
            protocol::RichTextContext::new_ordered(&self.token.owner.team, &self.token.owner.user)?;
        context.file(raw, label.clone())?;
        Ok(label)
    }
}

fn has_canvas_blocks(effective: &Value) -> bool {
    (effective["mode"] == "quip" || effective["filetype"] == "quip")
        && effective["title_blocks"]
            .as_array()
            .is_some_and(|blocks| !blocks.is_empty())
}

/// Local metadata writes already call this shared invalidation boundary. Keep
/// the predecessor for exact partial merges; only its current-read claim ends.
pub(crate) fn invalidate(shell: &mut ShellState, team: &str, id: &str) {
    if let Some(cache) = shell.profile_edit.sections.file_titles.as_mut()
        && cache.token.owner.team == team
        && (cache.reads.contains_key(id) || cache.changes.contains_key(id))
    {
        cache.reads.remove(id);
        cache.changes.insert(id.into(), Arc::new(()));
    }
}

fn request(context: &crate::scheduled::Context, id: &str) -> PreparedRequest {
    // 20Mt.fetchFileById defaults. Existing generic browser files.info has a
    // different shape and does not supply this original response contract.
    context.client.rest_form(
        &context.session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    )
}

pub(super) async fn recover(
    mut state: Signal<ShellState>,
    token: &Token,
    ids: &[String],
    force: bool,
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let context = context(&state.peek(), token).ok_or_else(unavailable)?;
    let generation = socket(&state.peek(), token).ok_or_else(unavailable)?;
    for id in ids {
        if id.len() < 2
            || id.len() > 255
            || !id.starts_with('F')
            || !id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(unavailable());
        }
        if !current(&state.peek()) || socket(&state.peek(), token) != Some(generation) {
            return Err(unavailable());
        }
        if !force && Observation::capture(&state.peek(), token, id).is_ok() {
            continue;
        }
        let revision = crate::file_events::snapshot_revision(&state.peek());
        let stamp = {
            let mut shell = state.write();
            let cache = cache(&mut shell, token)?;
            if cache.changes.len() >= 128 && !cache.changes.contains_key(id) {
                return Err(unavailable());
            }
            cache
                .changes
                .entry(id.clone())
                .or_insert_with(|| Arc::new(()))
                .clone()
        };
        let request = request(&context, id);
        let result = if crate::fixture::is_fixture() {
            fixture::files::read(state, &request).await
        } else {
            context.transport.execute(request).await
        };
        let mut shell = state.write();
        if !current(&shell)
            || socket(&shell, token) != Some(generation)
            || crate::file_events::changed_since(&shell, &token.owner.team, id, revision)
            || !read_current(&shell, token, id, &stamp)
        {
            return Err(unavailable());
        }
        let raw = result.map_err(|error| {
            let entry = shell
                .profile_edit
                .sections
                .entries
                .entry(token.owner.clone())
                .or_default();
            entry.retry_at = entry.retry_at.max(cooldown(&error));
            error
        })?;
        install(&mut shell, token, id, generation, revision, &raw)?;
    }
    Ok(())
}

fn read_current(shell: &ShellState, token: &Token, id: &str, stamp: &Arc<()>) -> bool {
    token.current(shell)
        && shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .is_some_and(|cache| {
                cache.token == *token
                    && cache
                        .changes
                        .get(id)
                        .is_some_and(|current| Arc::ptr_eq(current, stamp))
            })
}

/// Closed native fixture ingress; production cannot select this branch.
pub(crate) fn fixture_observe(shell: &mut ShellState, raw: &Value) -> Result<(), Error> {
    if !fixture::files::enabled(shell) {
        return Err(unavailable());
    }
    let token = Token::capture(shell).ok_or_else(unavailable)?;
    let id = raw["file"]["id"].as_str().ok_or_else(unavailable)?;
    let generation = socket(shell, &token).ok_or_else(unavailable)?;
    let revision = crate::file_events::snapshot_revision(shell);
    install(shell, &token, id, generation, revision, raw)
}

pub(crate) fn fixture_status(shell: &ShellState, id: &str) -> Value {
    if !fixture::files::enabled(shell) {
        return Value::Null;
    }
    let Some(cache) = &shell.profile_edit.sections.file_titles else {
        return Value::Null;
    };
    match cache.store.row(id) {
        Ok(row) => {
            let mut value = serde_json::json!({"source":row.source(),"effective":row.effective(),
                "current":Observation::capture(shell,&cache.token,id).is_ok()});
            if has_canvas_blocks(row.effective()) {
                value["quipTitle"] = match crate::canvas::quip_titles::Observation::capture(
                    shell, id,
                ) {
                    Some(observation)
                        if matches!(observation.value(), file_titles::QuipTitle::Nullish) =>
                    {
                        serde_json::json!({"current":observation.current(shell),"value":"nullish","fileId":id})
                    }
                    _ => Value::Null,
                };
            }
            value
        }
        Err(_) => Value::Null,
    }
}

fn install(
    shell: &mut ShellState,
    token: &Token,
    id: &str,
    generation: u64,
    revision: u64,
    raw: &Value,
) -> Result<(), Error> {
    if !token.current(shell)
        || socket(shell, token) != Some(generation)
        || raw["ok"] != true
        || raw["file"]["id"].as_str() != Some(id)
    {
        return Err(unavailable());
    }
    let cache = cache(shell, token)?;
    // Native adaptation uses a current singleton receipt; it does not mint a
    // complete global file inventory or reset a poisoned predecessor.
    let result = cache.store.ingest(raw).map_err(|_| unavailable());
    cache.reads.remove(id);
    cache.changes.insert(id.into(), Arc::new(()));
    result?;
    cache.reads.insert(id.into(), (generation, revision));
    Ok(())
}

fn cache<'a>(shell: &'a mut ShellState, token: &Token) -> Result<&'a mut Cache, Error> {
    if !token.current(shell) {
        return Err(unavailable());
    }
    let slot = &mut shell.profile_edit.sections.file_titles;
    if slot.as_ref().is_none_or(|cache| cache.token != *token) {
        *slot = Some(Cache {
            token: token.clone(),
            store: source::Store::new(),
            reads: BTreeMap::new(),
            changes: BTreeMap::new(),
        });
    }
    Ok(slot.as_mut().unwrap())
}
