//! Native canvas reading. Editing and collaborative updates need Slack's
//! separate document protocol; the reader never converts exports into writes.
use crate::{
    icons::{Icon, icon},
    state::{CanvasTarget, Overlay, ShellState},
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    canvas::{CanvasDocument, CanvasKind as K, CanvasNode as N},
    slack::{api, models::File},
};

pub fn open(mut state: Signal<ShellState>, file: File) {
    let Some(id) = file.id.as_ref().filter(|id| !id.is_empty()) else {
        state.write().show_toast("This canvas is not available.");
        return;
    };
    let target = CanvasTarget {
        id: id.clone(),
        title: super_platinum_core::state::file_title(&file),
        permalink: file
            .extra
            .get("permalink")
            .and_then(|v| v.as_str())
            .and_then(super_platinum_core::canvas::link_url),
    };
    open_target(state, target);
}
pub fn open_target(mut state: Signal<ShellState>, target: CanvasTarget) {
    let mut shell = state.write();
    if shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .is_some_and(|w| w.deleted_files.contains(&target.id))
    {
        shell.show_toast("This canvas has been deleted.");
        return;
    }
    shell.canvas = Some(target);
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    shell.overlay = Some(Overlay::Canvas);
}

#[derive(Default)]
struct Reader {
    document: Option<Arc<CanvasDocument>>,
    loading: bool,
    error: Option<String>,
    generation: u64,
    title: Option<String>,
    editable: bool,
}
async fn load(state: Signal<ShellState>, mut reader: Signal<Reader>, target: CanvasTarget) {
    let Some(ctx) = crate::scheduled::Context::capture(&state.peek()) else {
        reader.write().error = Some("Connect to your workspace to read this canvas.".into());
        return;
    };
    let generation = {
        let mut r = reader.write();
        r.generation = r.generation.wrapping_add(1);
        r.loading = true;
        r.error = None;
        r.generation
    };
    let response = api::fetch_canvas(&ctx.transport, &ctx.client, &ctx.session, &target.id).await;
    if !ctx.current(&state.peek()) || reader.peek().generation != generation {
        return;
    }
    let mut r = reader.write();
    r.loading = false;
    match response {
        Ok((file, doc)) => {
            r.title = Some(super_platinum_core::state::file_title(&file));
            r.editable = file
                .extra
                .get("editable")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            r.document = Some(Arc::new(doc));
        }
        Err(e) => {
            r.error = Some(
                if e.is_offline() {
                    "Reconnect to load this canvas. Your current view is still here."
                } else {
                    "This canvas could not load. Try again or open it in Slack."
                }
                .into(),
            )
        }
    }
}

#[component]
pub fn CanvasDialog(mut state: Signal<ShellState>, target: CanvasTarget) -> Element {
    let original = use_hook(|| target.clone());
    let reader = use_signal(|| {
        let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
        if crate::fixture::is_fixture() && original.id == "Ffixture" {
            Reader {
                document: (!matches!(fixture.as_str(), "canvas-loading" | "canvas-error")).then(
                    || {
                        Arc::new(
                            CanvasDocument::parse(include_str!("canvas_fixture.html"))
                                .expect("canvas fixture"),
                        )
                    },
                ),
                loading: fixture == "canvas-loading",
                error: (fixture == "canvas-error")
                    .then(|| "This canvas could not load. Try again or open it in Slack.".into()),
                generation: 0,
                title: Some(original.title.clone()),
                editable: fixture != "canvas-readonly",
            }
        } else {
            Reader::default()
        }
    });
    let initial = original.clone();
    use_future(move || {
        let initial = initial.clone();
        async move {
            if !crate::fixture::is_fixture() {
                load(state, reader, initial).await;
            }
        }
    });
    let mut rename = use_signal(
        || match std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() {
            Ok("canvas-rename" | "canvas-rename-error") => Some(RenameDraft {
                base: original.title.clone(),
                input: "Project handbook — autumn".into(),
                busy: false,
                error: (std::env::var("SUPER_PLATINUM_FIXTURE").as_deref()
                    == Ok("canvas-rename-error"))
                .then(|| "Could not confirm the new name. Your draft is still here.".into()),
            }),
            _ => None,
        },
    );
    let mut shown = use_signal(|| 80usize);
    let snapshot = reader.read();
    let loading = snapshot.loading;
    let title = snapshot
        .title
        .clone()
        .unwrap_or_else(|| target.title.clone());
    let rename_title = title.clone();
    let editing = rename.read().is_some();
    let rename_target = original.clone();
    let retry = original.clone();
    let refresh = original.clone();
    let permalink = original.permalink.clone();
    let epoch = state.read().media_epoch;
    rsx! {
        div {class:"scrim",onclick:move |_|state.write().dismiss_overlay()}
        section {id:"shell-overlay-dialog",class:"modal canvas-dialog",role:"dialog","aria-label":"Canvas: {title}","aria-modal":"true",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":state.read().shortcut_generation,
            header {
                div {class:"canvas-heading",span {class:"canvas-eyebrow","Canvas"}h2 {"{title}"}}
                button {class:"icon-btn","aria-label":"Close canvas",onclick:move |_|state.write().dismiss_overlay(),{icon(Icon::Close,"icon")}}
            }
            nav {class:"canvas-toolbar","aria-label":"Canvas actions",
                span {class:"canvas-status",if loading {"Loading…"} else {"Read view"}}
                span {class:"spacer"}
                if snapshot.editable {button {disabled:loading||editing,onclick:move |_|rename.set(Some(RenameDraft{base:rename_title.clone(),input:rename_title.clone(),busy:false,error:None})),"Rename"}}
                button {disabled:loading||editing,onclick:move |_|{spawn(load(state,reader,refresh.clone()));},"Refresh"}
                if let Some(url)=permalink {button {onclick:move |_|{if crate::media::open_external(&url).is_err(){state.write().show_toast("This canvas link could not be opened.");}},"Open in Slack"}}
            }
            if editing {RenameForm {state,reader,draft:rename,target:rename_target}}
            div {class:"canvas-scroll","aria-busy":loading,
                if let Some(error)=snapshot.error.as_ref() {div {class:"canvas-error",role:"alert",p {"{error}"}button {disabled:loading||editing,onclick:move |_|{spawn(load(state,reader,retry.clone()));},"Retry"}}}
                if let Some(document)=snapshot.document.as_ref() {
                    article {class:"canvas-document",
                        for node in document.nodes.iter().take(shown()) {{render_node(node,state,epoch)}}
                        if document.nodes.len()>shown() {button {class:"canvas-more",onclick:move |_|shown+=80,"Show more"}}
                    }
                } else if loading {
                    div {class:"canvas-skeleton",role:"status","aria-label":"Loading canvas",div {}div {}div {}div {}}
                }
            }
        }
    }
}

fn render_node(node: &N, state: Signal<ShellState>, epoch: u64) -> Element {
    let N::Element { kind, children, .. } = node else {
        let N::Text(text) = node else { unreachable!() };
        return rsx! {"{text}"};
    };
    let content = || rsx! {for child in children {{render_node(child,state,epoch)}}};
    match kind {
        K::Group => rsx! {div {{content()}}},
        K::Inline => rsx! {span {{content()}}},
        K::Paragraph => rsx! {p {{content()}}},
        K::Heading(1) => rsx! {h1 {{content()}}},
        K::Heading(2) => rsx! {h2 {{content()}}},
        K::Heading(3) => rsx! {h3 {{content()}}},
        K::Heading(_) => rsx! {h4 {{content()}}},
        K::Bold => rsx! {strong {{content()}}},
        K::Italic => rsx! {em {{content()}}},
        K::Strike => rsx! {s {{content()}}},
        K::Underline => rsx! {u {{content()}}},
        K::Code => rsx! {code {{content()}}},
        K::Pre => rsx! {pre {{content()}}},
        K::Quote => rsx! {blockquote {{content()}}},
        K::List {
            ordered: true,
            start,
            ..
        } => rsx! {ol {start:*start,{content()}}},
        K::List { checklist, .. } => {
            rsx! {ul {class:if *checklist {"canvas-checklist"}else{""},{content()}}}
        }
        K::Item {
            checked: Some(checked),
        } => {
            rsx! {li {class:"canvas-check-item",span {class:if *checked {"canvas-checkbox checked"}else{"canvas-checkbox"},role:"checkbox","aria-checked":*checked,"aria-disabled":"true","aria-label":if *checked {"Completed"}else{"Incomplete"},if *checked {{icon(Icon::Check,"icon")}}}div {{content()}}}}
        }
        K::Item { checked: None } => rsx! {li {{content()}}},
        K::Table => rsx! {div {class:"canvas-table-scroll",table {{content()}}}},
        K::TableHead => rsx! {thead {{content()}}},
        K::TableBody => rsx! {tbody {{content()}}},
        K::Row => rsx! {tr {{content()}}},
        K::Cell {
            heading: true,
            columns,
            rows,
        } => rsx! {th {colspan:*columns as i64,rowspan:*rows as i64,{content()}}},
        K::Cell { columns, rows, .. } => {
            rsx! {td {colspan:*columns as i64,rowspan:*rows as i64,{content()}}}
        }
        K::Link(url) => {
            let destination = url.clone();
            rsx! {a {href:"{url}",onclick:move |e|{e.prevent_default();let _=crate::media::open_external(&destination);},{content()}}}
        }
        K::Image {
            url: Some(url),
            alt,
        } => {
            let id = state.peek().media.register_image(
                super_platinum_core::MediaAssetKind::Attachment,
                url,
                "image/png",
            );
            rsx! {crate::view::media_image::MediaImage {state,id,media_epoch:epoch,alt:alt.clone(),class:"canvas-image",loading:"lazy"}}
        }
        K::Image { alt, .. } => rsx! {span {class:"canvas-image-unavailable","{alt}"}},
        K::Break => rsx! {br {}},
        K::Rule => rsx! {hr {}},
    }
}

#[derive(Clone)]
struct RenameDraft {
    base: String,
    input: String,
    busy: bool,
    error: Option<String>,
}

#[component]
fn RenameForm(
    state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<RenameDraft>>,
    target: CanvasTarget,
) -> Element {
    use_effect(|| {
        document::eval(
            "document.getElementById('canvas-name')?.focus(); document.getElementById('canvas-name')?.select();",
        );
    });
    let current = draft.read();
    let Some(value) = current.as_ref() else {
        return rsx! {};
    };
    let disabled = value.busy || value.input.trim().is_empty();
    let busy = value.busy;
    rsx! {
        form {class:"canvas-rename",onsubmit:move |event|{event.prevent_default();if !disabled {spawn(save_name(state,reader,draft,target.clone()));}},
            label {r#for:"canvas-name","Canvas name"}
            div {class:"canvas-rename-fields",
                input {id:"canvas-name",type:"text",initial_value:"{value.input}",disabled:busy,autocomplete:"off",oninput:move |event|{if let Some(d)=draft.write().as_mut(){d.input=event.value();d.error=None;}}}
                button {type:"button",disabled:busy,onclick:move |_|draft.set(None),"Cancel"}
                button {type:"submit",disabled,if busy {"Saving…"}else{"Save"}}
            }
            if let Some(error)=value.error.as_ref(){p {role:"alert","{error}"}}
        }
    }
}

async fn save_name(
    mut state: Signal<ShellState>,
    mut reader: Signal<Reader>,
    mut draft: Signal<Option<RenameDraft>>,
    target: CanvasTarget,
) {
    let Some(edit) = draft
        .peek()
        .as_ref()
        .filter(|d| !d.busy && !d.input.trim().is_empty())
        .cloned()
    else {
        return;
    };
    let visit = state.peek().shortcut_generation;
    let name = edit.input.trim().to_owned();
    if name == edit.base {
        draft.set(None);
        return;
    }
    let Some(ctx) = crate::scheduled::Context::capture(&state.peek()) else {
        if let Some(d) = draft.write().as_mut() {
            d.error = Some("Reconnect to rename this canvas. Your draft is still here.".into());
        }
        return;
    };
    if let Some(d) = draft.write().as_mut() {
        d.busy = true;
        d.error = None;
    }
    // Re-read permission and title immediately before sending. Slack's file
    // rename has no revision precondition; this detects an already stale draft,
    // but cannot provide atomic compare-and-set against simultaneous renames.
    let read_revision = crate::file_events::snapshot_revision(&state.peek());
    let current =
        api::fetch_canvas_file(&ctx.transport, &ctx.client, &ctx.session, &target.id).await;
    if !ctx.current(&state.peek())
        || state.peek().overlay != Some(Overlay::Canvas)
        || state.peek().shortcut_generation != visit
        || !state
            .peek()
            .canvas
            .as_ref()
            .is_some_and(|c| c.id == target.id)
    {
        return;
    }
    if crate::file_events::changed_since(
        &state.peek(),
        &ctx.session.team_id,
        &target.id,
        read_revision,
    ) {
        if let Some(d) = draft.write().as_mut() {
            d.busy = false;
            d.error = Some("This canvas changed while its name was being checked. Cancel and refresh to review the latest name.".into());
        }
        return;
    }
    let result = match current {
        Ok(file) => {
            let latest = super_platinum_core::state::file_title(&file);
            reader.write().title = Some(latest.clone());
            reader.write().editable = file
                .extra
                .get("editable")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            if latest == name {
                Ok(())
            } else {
                match api::validate_canvas_rename(&file, &edit.base)
                    .and_then(|()| api::canvas_rename(&ctx.client, &ctx.session, &target.id, &name))
                {
                    Ok(request) => ctx.transport.execute(request).await.map(|_| ()),
                    Err(error) => Err(error),
                }
            }
        }
        Err(error) => Err(error),
    };
    if !ctx.current(&state.peek())
        || state.peek().overlay != Some(Overlay::Canvas)
        || state.peek().shortcut_generation != visit
        || !state
            .peek()
            .canvas
            .as_ref()
            .is_some_and(|c| c.id == target.id)
    {
        return;
    }
    match result {
        Ok(()) => {
            if crate::file_events::changed_since(
                &state.peek(),
                &ctx.session.team_id,
                &target.id,
                read_revision,
            ) {
                // A newer remote snapshot already owns the cached name.
                draft.set(None);
                load(state, reader, target).await;
                return;
            }
            reader.write().title = Some(name.clone());
            {
                let mut shell = state.write();
                crate::file_events::invalidate(&mut shell, &ctx.session.team_id, &target.id);
                if let Some(canvas) = shell.canvas.as_mut().filter(|c| c.id == target.id) {
                    canvas.title = name.clone();
                }
                for file in shell.core.files.items.iter_mut() {
                    if file.id.as_deref() == Some(target.id.as_str()) {
                        file.title = Some(name.clone());
                    }
                }
                for file in shell.core.channel_files.items.iter_mut() {
                    if file.id.as_deref() == Some(target.id.as_str()) {
                        file.title = Some(name.clone());
                    }
                }
                shell.show_toast("Canvas renamed");
            }
            draft.set(None);
            load(state, reader, target).await;
        }
        Err(error) => {
            if let Some(d) = draft.write().as_mut() {
                d.busy = false;
                d.error=Some(match error {
                super_platinum_core::slack::Error::Api(ref code) if code=="canvas_title_changed"=>"The name changed elsewhere. Cancel to review the latest name, then try again.",
                super_platinum_core::slack::Error::Api(ref code) if code=="canvas_not_editable"=>"You no longer have permission to rename this canvas.",
                _ if error.is_offline()=>"Reconnect to rename this canvas. Your draft is still here.",
                _=>"Could not confirm the new name. Your draft is still here. Cancel and refresh to check it before trying again.",
            }.into());
            }
        }
    }
}
