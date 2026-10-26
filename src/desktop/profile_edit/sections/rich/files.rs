//! Profile opening/read custody over the shared credential-owned file source.
//! The local cache retains read stamps and fixture predecessors, not a second
//! authority capable of overriding a newer shared event.
use super::*;
use std::collections::BTreeMap;
mod channels;
mod groups;
mod metadata;
pub(crate) mod sections;
mod users;
use super_platinum_core::slack::{
    PreparedRequest,
    api::custom::texty::{file_title_source as source, file_titles},
};
pub(super) use users::acquire_policy;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(in crate::profile_edit::sections) struct Cache {
    token: Token,
    store: source::Store,
    reads: BTreeMap<String, (u64, u64)>,
    changes: BTreeMap<String, Arc<()>>,
    policy: Option<Arc<users::Policy>>,
}

#[derive(Clone)]
pub(super) struct Observation {
    token: Token,
    id: String,
    row: source::ReferenceObservation,
    metadata: metadata::Metadata,
    quip: Option<crate::canvas::quip_titles::Observation>,
    socket: u64,
    revision: u64,
    members: Option<users::Members>,
    channels: Option<channels::Channels>,
    groups: Option<groups::Groups>,
    emoji: super::title_locale::EmojiTitle,
    section: Option<sections::Preview>,
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
        let metadata = metadata::Metadata::capture(shell, token, id)?;
        let (generation, revision) = (metadata.socket, metadata.revision);
        let row = metadata.row.clone();
        // File metadata does not establish the independent editor-title state.
        // Only block-title Canvas labels consume that runtime observation.
        let dependencies = reference_dependencies(&row)?;
        let members = users::Members::capture(shell, token, &dependencies.users)?;
        let channels = channels::Channels::capture(shell, token, &dependencies.channels)?;
        let groups = groups::Groups::capture(shell, token, &dependencies.groups)?;
        let quip = if matches!(row.value(), source::ReferenceValue::Normal(_))
            && has_canvas_blocks(row.effective())
            && dependencies.users.is_empty()
        {
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
            metadata,
            quip,
            socket: generation,
            revision,
            members,
            channels,
            groups,
            emoji: super::title_locale::EmojiTitle::english(),
            section: None,
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.section
            .as_ref()
            .is_none_or(|s| s.current(shell, &self.token))
            && socket(shell, &self.token) == Some(self.socket)
            && self
                .members
                .as_ref()
                .is_none_or(|m| m.current(shell, &self.token))
            && self.quip.as_ref().is_none_or(|quip| quip.current(shell))
            && self
                .channels
                .as_ref()
                .is_none_or(|c| c.current(shell, &self.token))
            && self
                .groups
                .as_ref()
                .is_none_or(|g| g.current(shell, &self.token))
            && self.metadata.current(shell, &self.token)
    }
    pub(super) fn label(&self, raw: &Value) -> Result<String, Error> {
        if raw["file_id"].as_str() != Some(self.id.as_str()) {
            return Err(unavailable());
        }
        if self.private_canvas() || self.deleted_canvas() {
            if raw["type"] != "canvas" {
                return Err(unavailable());
            }
            let label = if self.deleted_canvas() {
                super::title_locale::DeletedCanvas::english().value()
            } else {
                super::title_locale::PrivateCanvas::english().value()
            }
            .to_owned();
            let mut context = protocol::RichTextContext::new_ordered(
                &self.token.owner.team,
                &self.token.owner.user,
            )?;
            context.file(raw, label.clone())?;
            return Ok(label);
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
        let label = file_titles::label_with_group_resolver(
            kind,
            file_titles::Status::Normal,
            effective,
            self.quip
                .as_ref()
                .map_or(file_titles::QuipTitle::Unavailable, |quip| quip.value()),
            Some(super::title_locale::CanvasTitle::english().value()),
            Some(self.emoji.context()),
            |id| {
                self.members
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
            |id| {
                self.channels
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
            |id| {
                self.groups
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
        )
        .map_err(|_| unavailable())?;
        // The same closed whole-token grammar is used for opening and capture.
        let mut context =
            protocol::RichTextContext::new_ordered(&self.token.owner.team, &self.token.owner.user)?;
        context.file(raw, label.clone())?;
        Ok(label)
    }
    pub(super) fn private_canvas(&self) -> bool {
        matches!(self.row.value(), source::ReferenceValue::PrivateCanvas)
    }
    pub(super) fn deleted_canvas(&self) -> bool {
        matches!(self.row.value(), source::ReferenceValue::DeletedCanvas)
    }
    pub(super) fn channel_ids(&self) -> impl Iterator<Item = &String> {
        self.channels
            .iter()
            .flat_map(|channels| channels.ids())
            .chain(
                self.section
                    .iter()
                    .flat_map(|section| section.channel_ids()),
            )
    }
}

fn reference_dependencies(
    row: &source::ReferenceObservation,
) -> Result<file_titles::TitleDependencies, Error> {
    match row.value() {
        source::ReferenceValue::Normal(effective) => title_dependencies(effective),
        source::ReferenceValue::PrivateCanvas | source::ReferenceValue::DeletedCanvas => {
            Ok(file_titles::TitleDependencies::default())
        }
    }
}
fn title_user_ids(effective: &Value) -> Result<std::collections::BTreeSet<String>, Error> {
    Ok(title_dependencies(effective)?.users)
}
fn title_dependencies(effective: &Value) -> Result<file_titles::TitleDependencies, Error> {
    if !has_canvas_blocks(effective) {
        return Ok(file_titles::TitleDependencies {
            users: Default::default(),
            channels: Default::default(),
            groups: Default::default(),
        });
    }
    file_titles::dependencies_with_groups(
        effective,
        Some(super::title_locale::EmojiTitle::english().context()),
    )
    .map_err(|_| unavailable())
}
fn has_canvas_blocks(effective: &Value) -> bool {
    (effective["mode"] == "quip" || effective["filetype"] == "quip")
        && effective["title_blocks"]
            .as_array()
            .is_some_and(|blocks| !blocks.is_empty())
}

/// File receipt readiness is separate from nested member/policy display custody.
fn metadata_current(shell: &ShellState, token: &Token, id: &str) -> bool {
    metadata::Metadata::capture(shell, token, id).is_ok()
}
fn reuse_metadata(shell: &ShellState, token: &Token, id: &str, force: bool) -> bool {
    metadata_current(shell, token, id)
        && (metadata::Metadata::capture(shell, token, id).is_ok_and(|m| m.row.is_event_deletion())
            || !force
            || dependencies(shell, token, &[id.to_owned()])
                .is_ok_and(|v| !v.ids.is_empty() || !v.channels.is_empty() || !v.groups.is_empty()))
}
pub(super) struct Dependencies {
    pub(super) ids: Vec<String>,
    pub(super) channels: std::collections::BTreeSet<String>,
    pub(super) groups: std::collections::BTreeSet<String>,
    rows: Vec<metadata::Metadata>,
    sections: Vec<super::search::file_source::sections::Observation>,
}
impl Dependencies {
    pub(super) fn current(&self, shell: &ShellState, token: &Token) -> bool {
        self.rows.iter().all(|row| row.current(shell, token))
            && self.sections.iter().all(|row| row.current(shell))
    }
}
pub(super) fn dependencies(
    shell: &ShellState,
    token: &Token,
    ids: &[String],
) -> Result<Dependencies, Error> {
    let mut users = std::collections::BTreeSet::new();
    let mut channels = std::collections::BTreeSet::new();
    let mut groups = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for id in ids {
        let metadata = metadata::Metadata::capture(shell, token, id)?;
        let needed = reference_dependencies(&metadata.row)?;
        users.extend(needed.users);
        channels.extend(needed.channels);
        groups.extend(needed.groups);
        rows.push(metadata);
    }
    Ok(Dependencies {
        ids: users.into_iter().collect(),
        channels,
        groups,
        rows,
        sections: Vec::new(),
    })
}

/// Schedule each admitted nested selector encounter after releasing the shell
/// borrow, including when another dependency prevents this opening.
pub(super) fn channel_hits(
    shell: &mut ShellState,
    token: &Token,
    dependencies: &Dependencies,
) -> Vec<opening::Hit> {
    if !dependencies.current(shell, token) || dependencies.channels.is_empty() {
        return Vec::new();
    }
    let Ok(source) = canonical::observed(shell, &token.owner.team) else {
        return Vec::new();
    };
    dependencies
        .channels
        .iter()
        .filter_map(|id| {
            if source.row(id).is_some_and(|row| {
                file_titles::known_ordinary_channel_label_with_context(
                    id,
                    row,
                    Some(super::title_locale::ChannelTitle::english().context()),
                )
                .is_ok()
            }) {
                return Some(opening::Hit::Channel {
                    source: source.clone(),
                    id: id.clone(),
                });
            }
            crate::app_home::channel_fetches::unknown::select(shell, &token.owner.team, id)
                .ok()
                .map(opening::Hit::MissingChannel)
        })
        .collect()
}

/// Existing shared group history, not a typed cache entry, owns this selector hit.
pub(super) fn group_hits(
    shell: &mut ShellState,
    token: &Token,
    dependencies: &Dependencies,
) -> Vec<opening::Hit> {
    if !dependencies.current(shell, token) || dependencies.groups.is_empty() {
        return vec![];
    }
    let Ok(policy) = groups::Policy::capture(shell, token) else {
        return vec![];
    };
    if policy.restricted() {
        return vec![];
    }
    dependencies
        .groups
        .iter()
        .filter_map(|id| {
            crate::usergroups::unknown::select(shell, &token.owner.team, id)
                .ok()
                .map(opening::Hit::MissingGroup)
        })
        .collect()
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
        if reuse_metadata(&state.peek(), token, id, force) {
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
        let shared_read = metadata::read(&mut state.write(), token, id)?;
        let request = request(&context, id);
        let owns = || {
            let shell = state.peek();
            current(&shell)
                && socket(&shell, token) == Some(generation)
                && !crate::file_events::changed_since(&shell, &token.owner.team, id, revision)
                && read_current(&shell, token, id, &stamp)
                && shared_read.as_ref().is_none_or(|read| read.current(&shell))
        };
        let result = if crate::fixture::is_fixture() {
            fixture::files::read(state, &request).await.and_then(|raw| {
                source::FileReferenceResponse::decode(
                    id,
                    &serde_json::to_vec(&raw).map_err(|_| unavailable())?,
                )
            })
        } else {
            context
                .transport
                .execute_file_reference_current(request, owns)
                .await
        };
        let mut shell = state.write();
        if !current(&shell)
            || socket(&shell, token) != Some(generation)
            || crate::file_events::changed_since(&shell, &token.owner.team, id, revision)
            || !read_current(&shell, token, id, &stamp)
            || shared_read
                .as_ref()
                .is_some_and(|read| !read.current(&shell))
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
        install_response_with_read(
            &mut shell,
            token,
            id,
            generation,
            revision,
            &raw,
            shared_read,
        )?;
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

/// The bounded fixture drives the same read/receipt path while a mounted editor
/// retains its own Catalog. No source/cache marker is injected.
pub(crate) async fn fixture_refresh(state: Signal<ShellState>) -> Result<(), Error> {
    if !(fixture::unavailable_reference::named()
        || fixture::deleted_reference::named()
        || fixture::deletion_event::named())
        || !fixture::files::enabled(&state.peek())
    {
        return Err(unavailable());
    }
    let token = Token::capture(&state.peek()).ok_or_else(unavailable)?;
    recover(state, &token, &[fixture::files::ID.into()], true, |shell| {
        token.current(shell)
    })
    .await
}
pub(crate) fn fixture_status(shell: &ShellState, id: &str) -> Value {
    if !fixture::files::enabled(shell) {
        return Value::Null;
    }
    let Some(token) = Token::capture(shell) else {
        return Value::Null;
    };
    let observed = metadata::Metadata::capture(shell, &token, id)
        .map(|m| m.row)
        .or_else(|_| {
            shell
                .profile_edit
                .sections
                .file_titles
                .as_ref()
                .ok_or_else(unavailable)?
                .store
                .reference(id)
        });
    match observed {
        Ok(row) => {
            let mut value = serde_json::json!({"source":row.source(),"effective":row.effective(),
                "current":Observation::capture(shell,&token,id).is_ok()});
            if matches!(row.value(), source::ReferenceValue::DeletedCanvas) {
                value["reference"] = serde_json::json!({"status":"deleted-canvas","receipt":row.receipt(),
                    "presentation":super::title_locale::DeletedCanvas::english().descriptor(),
                    "dependencies":{"users":[],"channels":[],"groups":[]},"quip":null});
                return value;
            }
            if matches!(row.value(), source::ReferenceValue::PrivateCanvas) {
                value["reference"] = serde_json::json!({"status":"private-canvas","receipt":row.receipt(),
                    "presentation":super::title_locale::PrivateCanvas::english().descriptor(),
                    "dependencies":{"users":[],"channels":[],"groups":[]},"quip":null});
                return value;
            }
            if row.effective()["mode"] == "quip" || row.effective()["filetype"] == "quip" {
                value["canvasFallback"] = super::title_locale::CanvasTitle::english()
                    .proof(Observation::capture(shell, &token, id).is_ok());
                value["emojiText"] = super::title_locale::EmojiTitle::english()
                    .proof(Observation::capture(shell, &token, id).is_ok());
                if let Ok(observed) = Observation::capture(shell, &token, id)
                    && let Some(channels) = observed.channels.as_ref()
                {
                    value["titleChannels"] = channels.snapshot();
                }
            }
            if let Ok(observed) = Observation::capture(shell, &token, id)
                && let Some(groups) = observed.groups.as_ref()
            {
                value["titleGroups"] = groups.snapshot();
            }
            if let Ok(Some(members)) = title_user_ids(row.effective())
                .and_then(|ids| users::Members::capture(shell, &token, &ids))
            {
                value["titleUsers"] = members.snapshot();
            }
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
    let response = source::FileReferenceResponse::decode(
        id,
        &serde_json::to_vec(raw).map_err(|_| unavailable())?,
    )?;
    install_response(shell, token, id, generation, revision, &response)
}
fn install_response(
    shell: &mut ShellState,
    token: &Token,
    id: &str,
    generation: u64,
    revision: u64,
    response: &source::FileReferenceResponse,
) -> Result<(), Error> {
    let shared_read = metadata::read(shell, token, id)?;
    install_response_with_read(
        shell,
        token,
        id,
        generation,
        revision,
        response,
        shared_read,
    )
}
fn install_response_with_read(
    shell: &mut ShellState,
    token: &Token,
    id: &str,
    generation: u64,
    revision: u64,
    response: &source::FileReferenceResponse,
    shared_read: Option<super::search::file_source::reference::ReferenceRead>,
) -> Result<(), Error> {
    if !token.current(shell) || socket(shell, token) != Some(generation) || response.id() != id {
        return Err(unavailable());
    }
    let shared_store = shared_read
        .map(|read| read.publish(shell, response))
        .transpose()?;
    let cache = cache(shell, token)?;
    // This typed result certifies the exact newly committed receipt, never an
    // old category left behind by a malformed/error no-op ingestion.
    let observed = if let Some(store) = shared_store {
        cache.store = store;
        cache.store.reference(id).map_err(|_| unavailable())
    } else {
        cache
            .store
            .ingest_reference(&token.owner.team, response)
            .map_err(|_| unavailable())
    };
    cache.reads.remove(id);
    cache.changes.insert(id.into(), Arc::new(()));
    let observed = observed?;
    if !observed.current(&cache.store) || observed.receipt() != response.raw() {
        return Err(unavailable());
    }
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
            policy: None,
        });
    }
    Ok(slot.as_mut().unwrap())
}
