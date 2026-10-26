//! Profile's explicit awaited missing-snippet ensure. Conversion is no-fetch;
//! current absence/error mounts an ellipsis and a later opening may try again.
use super::super::search::file_source::sections as shared;
use super::*;
use crate::directory_member_fetches::Context;
use super_platinum_core::slack::api::CanvasSectionsRequest;
mod preview;
pub(super) use preview::Preview;
pub(in crate::profile_edit::sections::rich) use preview::Required;

pub(in crate::profile_edit::sections::rich) fn id(raw: &Value) -> Option<&str> {
    (raw["type"] == "canvas")
        .then(|| raw["section_id"].as_str().filter(|id| !id.is_empty()))
        .flatten()
}
pub(in crate::profile_edit::sections::rich) fn needs(
    shell: &ShellState,
    token: &Token,
    blocks: &[Value],
) -> bool {
    let Ok(tokens) = collect(blocks) else {
        return false;
    };
    tokens.into_iter().any(|raw| {
        let Some(section) = id(raw) else { return false };
        let Some(file) = raw["file_id"].as_str() else {
            return false;
        };
        if metadata::Metadata::capture(shell, token, file)
            .is_ok_and(|m| !matches!(m.row.value(), source::ReferenceValue::Normal(_)))
        {
            return false;
        }
        !shared::Observation::capture(shell, &token.owner.team, file, section)
            .is_ok_and(|row| row.row().is_cached())
    })
}

pub(in crate::profile_edit::sections::rich) async fn ensure(
    state: Signal<ShellState>,
    token: &Token,
    blocks: &[Value],
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for raw in collect(blocks)? {
        let Some(section) = id(raw) else { continue };
        let file = raw["file_id"].as_str().ok_or_else(unavailable)?;
        let ids = groups.entry(file.into()).or_default();
        if !ids.iter().any(|id| id == section) {
            ids.push(section.into());
        }
    }
    if groups.is_empty() {
        return Ok(());
    }
    let owner = Context::capture(&state.peek(), &token.owner.team)?;
    for (file, ids) in groups {
        if !current(&state.peek()) || !owner.current(&state.peek()) {
            return Err(unavailable());
        }
        let metadata = metadata::Metadata::capture(&state.peek(), token, &file)?;
        // Existing event/API status policy overrides the label and refuses a
        // new section acquisition. It does not fabricate a missing snippet.
        if !matches!(metadata.row.value(), source::ReferenceValue::Normal(_)) {
            continue;
        }
        let missing = ids
            .into_iter()
            .filter(|id| {
                !shared::Observation::capture(&state.peek(), &token.owner.team, &file, id)
                    .is_ok_and(|row| row.row().is_cached())
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            continue;
        }
        acquire(state, token, &owner, &metadata, &file, &missing, &current).await?;
    }
    Ok(())
}
async fn acquire(
    mut state: Signal<ShellState>,
    token: &Token,
    owner: &Context,
    metadata: &metadata::Metadata,
    file: &str,
    ids: &[String],
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let request = CanvasSectionsRequest::new(file, ids)?;
    let read = shared::Read::capture(&mut state.write(), &token.owner.team, request)?;
    let prepared = read.request().prepare(&owner.client, &owner.session);
    let owns = || {
        current(&state.peek())
            && owner.current(&state.peek())
            && metadata.current(&state.peek(), token)
            && read.current(&state.peek())
    };
    if !owns() {
        return Err(unavailable());
    }
    let result = if crate::fixture::is_fixture() {
        fixture::section_reference::read(state, &prepared, read.request()).await
    } else {
        owner.transport.execute_current(prepared, owns).await
    };
    if !owns() {
        return Err(unavailable());
    }
    let response = match result {
        Ok(raw) => read.request().decode(raw),
        Err(_) => read.request().failed(),
    };
    read.publish(&mut state.write(), &response)?;
    Ok(())
}

pub(crate) fn fixture_status(shell: &ShellState) -> Value {
    if !fixture::section_reference::named() {
        return Value::Null;
    }
    let Some(d) = shell.profile_edit.sections.draft.as_ref() else {
        return Value::Null;
    };
    let rows = fixture::section_reference::IDS.iter().map(|id| {
        let row = shared::Observation::capture(shell, &d.token.owner.team, fixture::files::ID, id);
        match row {
            Ok(row) => serde_json::json!({"id":id,"current":row.current(shell),"hasEntry":row.row().has_entry(),"cached":row.row().is_cached(),"block":row.row().block(),
                "previewDependencies":Preview::capture(shell,&d.token,row.clone()).ok().map(|p|p.snapshot())}),
            Err(_) => serde_json::json!({"id":id,"current":false}),
        }
    }).collect::<Vec<_>>();
    serde_json::json!({"rows":rows})
}
/// Closed fixture drives another source consumer through the identical guarded
/// request/publication path. It does not reset or patch the cache.
pub(crate) async fn fixture_refresh(state: Signal<ShellState>) -> Result<(), Error> {
    if !fixture::section_reference::named() {
        return Err(unavailable());
    }
    let token = state
        .peek()
        .profile_edit
        .sections
        .draft
        .as_ref()
        .ok_or_else(unavailable)?
        .token
        .clone();
    let owner = Context::capture(&state.peek(), &token.owner.team)?;
    let metadata = metadata::Metadata::capture(&state.peek(), &token, fixture::files::ID)?;
    acquire(
        state,
        &token,
        &owner,
        &metadata,
        fixture::files::ID,
        &fixture::section_reference::IDS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>(),
        &|shell| token.current(shell),
    )
    .await
}

impl Observation {
    pub(in crate::profile_edit::sections::rich) fn section_label(
        &mut self,
        shell: &ShellState,
        raw: &Value,
    ) -> Result<Vec<u16>, Error> {
        let section = id(raw).ok_or_else(unavailable)?;
        let source =
            shared::Observation::capture(shell, &self.token.owner.team, &self.id, section)?;
        let mut whole = raw.clone();
        whole
            .as_object_mut()
            .ok_or_else(unavailable)?
            .remove("section_id");
        let name = self.label(&whole)?;
        let observed = Preview::capture(shell, &self.token, source)?;
        let label = file_titles::section_label_with_resolver(
            &self.row,
            observed.row(),
            &name,
            file_titles::CanvasSectionText::english(),
            |leaf| observed.label(leaf),
        )
        .map_err(|_| unavailable())?;
        let units = label.units().to_vec();
        let mut context =
            protocol::RichTextContext::new_ordered(&self.token.owner.team, &self.token.owner.user)?;
        context.canvas_section(raw, &units)?;
        self.section = Some(observed);
        Ok(units)
    }
}
