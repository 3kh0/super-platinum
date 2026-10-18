//! Native canvas reading. Editing and collaborative updates need Slack's
//! separate document protocol; the reader never converts exports into writes.
pub(crate) mod checklist;
pub(crate) mod comments;
mod loader;
use loader::load;
pub(crate) mod surface;
pub(crate) mod text;

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
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::Canvas(target.clone()),
    ) {
        return;
    }
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
    checklist::open_visit(&mut shell);
}

#[derive(Default)]
struct Reader {
    document: Option<Arc<CanvasDocument>>,
    loading: bool,
    error: Option<String>,
    generation: u64,
    title: Option<String>,
    editable: bool,
    canonical_loading: bool,
    checklist: checklist::reader::Mapping,
    text: text::Mapping,
}
impl Reader {
    fn mapping_needed(&self, owner: Option<(u64, u64)>, loading: bool) -> bool {
        self.checklist.owner != owner || (self.canonical_loading && !loading)
    }

    fn finish_export(&mut self, generation: u64, canonical_current: bool) -> bool {
        if self.generation != generation {
            return false;
        }
        self.loading = false;
        canonical_current
    }
}

#[component]
pub fn CanvasDialog(
    mut state: Signal<ShellState>,
    target: CanvasTarget,
    #[props(default = false)] embedded: bool,
) -> Element {
    let original = use_hook(|| target.clone());
    let local_generation = use_hook(|| state.peek().shortcut_generation);
    let mut reader = use_signal(|| {
        let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
        if crate::fixture::is_fixture()
            && (original.id == "Ffixture" || surface::fixture_target(&original))
        {
            Reader {
                document: (!matches!(fixture.as_str(), "canvas-loading" | "canvas-error")).then(
                    || {
                        if comments::annotation::fixture::requested() {
                            return Arc::new(comments::annotation::fixture::document());
                        }
                        Arc::new(
                            CanvasDocument::parse(if text::structure_fixture::requested() {
                                text::structure_fixture::HTML
                            } else if text::fixture::requested() {
                                text::fixture::HTML
                            } else if checklist::fixture::requested() {
                                checklist::fixture::HTML
                            } else {
                                include_str!("canvas_fixture.html")
                            })
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
                canonical_loading: false,
                checklist: Default::default(),
                text: Default::default(),
            }
        } else {
            Reader::default()
        }
    });
    let comments = use_signal(comments::State::default);
    let mut comments_open = use_signal(|| {
        comments::fixture::requested() && !comments::annotation::fixture::inline_requested()
    });
    comments::focus::use_focus(state, comments, comments_open);
    use_future(move || async move {
        if comments::fixture::requested() && !comments::annotation::fixture::inline_requested() {
            comments::open(state, comments);
        }
    });
    // Fixture/backend completions can update canonical state without owning the
    // component's Reader signal. Keep that signal local (ShellState is Send).
    // The owner token makes ordinary shell ticks an O(1) check, not a reproject.
    use_effect(move || {
        let shell = state.read();
        let view = checklist::presentation(&shell);
        let owner = view.as_ref().map(|view| (view.visit, view.read));
        let loading = view.as_ref().is_some_and(|view| view.loading);
        if reader.peek().mapping_needed(owner, loading) {
            checklist::reader::update_mapping(&shell, &mut reader.write());
        }
    });
    use_effect(move || {
        if loader::fixture::pending(&state.read()) {
            loader::fixture::tick(&mut state.write(), &mut reader.write());
        }
    });
    let initial = original.clone();
    use_future(move || {
        let initial = initial.clone();
        async move {
            if embedded {
                if !surface::mount(&mut state.write(), &initial) {
                    return;
                }
            }
            if comments::annotation::fixture::inline_requested() {
                comments::annotation::fixture::refresh_reader(
                    &mut state.write(),
                    &mut reader.write(),
                );
            } else if text::structure_fixture::requested() {
                let mut shell = state.write();
                text::structure_fixture::install(&mut shell);
                checklist::reader::update_mapping(&shell, &mut reader.write());
            } else if text::fixture::requested() {
                let mut shell = state.write();
                text::fixture::install(&mut shell);
                checklist::reader::update_mapping(&shell, &mut reader.write());
                loader::fixture::start(&mut shell, &mut reader.write());
            } else if checklist::fixture::requested() {
                let mut shell = state.write();
                checklist::fixture::install(&mut shell);
                checklist::reader::update_mapping(&shell, &mut reader.write());
            } else if !crate::fixture::is_fixture() {
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
    let mut text_draft = use_signal(|| None::<text::Draft>);
    let local_review = use_signal(|| None::<checklist::recovery::review::Draft>);
    use_effect(move || {
        if rename.read().is_none()
            && text_draft.read().is_none()
            && state.peek().canvas_checklist.local_editor == Some(local_generation)
        {
            surface::release_local(&mut state.write(), local_generation);
        }
    });
    use_effect(move || {
        let shell = state.read();
        if text::should_close(&shell, text_draft.peek().as_ref()) {
            text_draft.set(None);
        }
    });
    // File events can arrive after an accepted save's readback has settled.
    // Coalesce revisions into one component-owned read task, with at most one
    // dirty followup. No timer runs when the view has no new file event.
    let mut event_refreshing = use_signal(|| false);
    use_effect(move || {
        let visit = {
            let shell = state.read();
            if crate::fixture::is_fixture()
                || *event_refreshing.read()
                || reader.read().loading
                || rename.read().is_some()
                || shell.core.transport.as_ref().is_none_or(|transport| {
                    transport.health() == super_platinum_core::slack::transport::Health::Offline
                })
            {
                None
            } else {
                checklist::event_reads::needed(&shell)
            }
        };
        let Some(visit) = visit else { return };
        event_refreshing.set(true);
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let read = {
                let mut shell = state.write();
                if reader.peek().loading
                    || rename.peek().is_some()
                    || shell.core.transport.as_ref().is_none_or(|transport| {
                        transport.health() == super_platinum_core::slack::transport::Health::Offline
                    })
                {
                    None
                } else {
                    checklist::event_reads::claim(&mut shell, visit)
                }
            };
            if let Some(read) = read {
                loader::read_pair(state, reader, read).await;
            }
            event_refreshing.set(false);
        });
    });
    let mut shown = use_signal(|| 80usize);
    let snapshot = reader.read();
    let canonical = checklist::presentation(&state.read());
    let loading = snapshot.loading;
    let refreshing = loading || canonical.as_ref().is_some_and(|view| view.loading);
    let notice = canonical
        .as_ref()
        .filter(|view| {
            snapshot.document.is_some()
                || snapshot.checklist.exported > 0
                || !snapshot.text.items.is_empty()
                || view.operation.is_some()
        })
        .and_then(checklist::reader::notice);
    let no_mapping = checklist::reader::Mapping::default();
    let mapped = if canonical.is_some() {
        &snapshot.checklist
    } else {
        &no_mapping
    };
    let title = snapshot
        .title
        .clone()
        .unwrap_or_else(|| target.title.clone());
    let rename_title = title.clone();
    let editing = rename.read().is_some()
        || text_draft.read().is_some()
        || local_review.read().is_some()
        || *comments_open.read();
    let block_refresh = *comments_open.read()
        || local_review.read().is_some()
        || rename.read().is_some()
        || text_draft
            .read()
            .as_ref()
            .is_some_and(|draft| draft.pending.is_none());
    let checklist_pending = canonical
        .as_ref()
        .is_some_and(|view| view.operation.is_some());
    let writing = canonical.as_ref().is_some_and(|view| {
        view.operation
            .as_ref()
            .is_some_and(|(_, _, stage)| stage.blocks_refresh())
    });
    let review_target = original.clone();
    let local_review_available = checklist::recovery::review::available(&state.read());
    let rename_target = original.clone();
    let retry = original.clone();
    let refresh = original.clone();
    let permalink = original.permalink.clone();
    let epoch = state.read().media_epoch;
    let unavailable = embedded && !surface::visible(&state.read(), &original.id);
    rsx! {
        if !embedded {div {class:"scrim",onclick:move |_|state.write().dismiss_overlay()}}
        section {id:if embedded {"channel-canvas-panel"}else{"shell-overlay-dialog"},class:if embedded {"canvas-dialog channel-canvas-panel"}else{"modal canvas-dialog"},role:if embedded {"region"}else{"dialog"},"aria-label":"Canvas: {title}","aria-modal":if embedded {None}else{Some("true")},tabindex:"-1","data-keyboard-dialog":if embedded {None}else{Some("true")},"data-dialog-kind":"other","data-dialog-generation":state.read().shortcut_generation,
            onmounted:move |_|if embedded {surface::focus(state.peek().shortcut_generation);},
            header {
                div {class:"canvas-heading",span {class:"canvas-eyebrow","Canvas"}h2 {"{title}"}}
                if embedded {button {class:"canvas-more",onclick:move |_|crate::channel_resources::select(state,crate::state::ChannelTab::Messages),"Messages"}}
                else {button {class:"icon-btn","aria-label":"Close canvas",onclick:move |_|state.write().dismiss_overlay(),{icon(Icon::Close,"icon")}}}
            }
            nav {class:"canvas-toolbar","aria-label":"Canvas actions",
                span {class:"canvas-status",if text_draft.read().is_some() {"Editing paragraph"} else if loading {"Loading…"} else {"Read view"}}
                span {class:"spacer"}
                button {id:"canvas-comment-selection",disabled:editing||writing||loading||*comments_open.read(),
                    onmousedown:|event|event.prevent_default(),
                    onclick:move |_|comments::annotation::range::choose(state,reader,comments,comments_open),"Comment on selection"}
                button {id:"canvas-comments-open","data-canvas-comment-action":"open",disabled:editing||writing,onclick:move |_|{comments_open.set(true);comments::open(state,comments);},"Show all threads"}
                if snapshot.editable {button {disabled:refreshing||editing||checklist_pending||canonical.as_ref().is_some_and(|view|view.stale),onclick:move |_|{if text_draft.peek().is_none() && checklist::presentation(&state.peek()).is_none_or(|view|view.operation.is_none()&&!view.loading&&!view.stale) && !reader.peek().loading {if !surface::hold_local(&mut state.write(),local_generation) {return;}rename.set(Some(RenameDraft{base:rename_title.clone(),input:rename_title.clone(),busy:false,error:None}));}},"Rename"}}
                if local_review_available {button {disabled:refreshing||editing,onclick:move |_|checklist::recovery::review::open(state,reader,review_target.clone(),local_review),"Review saved change"}}
                button {disabled:refreshing||block_refresh||writing,onclick:move |_|{spawn(load(state,reader,refresh.clone()));},"Refresh"}
                if let Some(url)=permalink {button {onclick:move |_|{if crate::media::open_external(&url).is_err(){state.write().show_toast("This canvas link could not be opened.");}},"Open in Slack"}}
            }
            if unavailable {p {class:"canvas-status",role:"status","This tab is no longer available. Your edit is kept here; copy it or cancel before changing tabs."}}
            if let Some(notice) = notice {p {class:"canvas-status",role:"status", "{notice}"}}
            if rename.read().is_some() {RenameForm {state,reader,draft:rename,target:rename_target}}
            if *comments_open.read() {comments::Panel {state,model:comments,opened:comments_open}} else {
            div {class:"canvas-scroll","aria-busy":loading,
                if local_review.read().is_some() {checklist::recovery::review::Panel {state,draft:local_review}}
                if let Some(error)=snapshot.error.as_ref() {div {class:"canvas-error",role:"alert",p {"{error}"}button {disabled:refreshing||block_refresh||writing,onclick:move |_|{spawn(load(state,reader,retry.clone()));},"Retry"}}}
                if let Some(generation)=text_draft.read().as_ref().map(|draft|draft.generation) {
                    text::Editor {key:"canvas-text-{generation}",state,reader,draft:text_draft}
                }
                if let Some(document)=snapshot.document.as_ref() {
                    article {class:"canvas-document",
                        for node in document.nodes.iter().take(shown()) {{render_node(node,state,epoch,mapped,reader,canonical.as_ref(),rename,text_draft,None,comments,comments_open,local_review)}}
                        if document.nodes.len()>shown() {button {class:"canvas-more",onclick:move |_|shown+=80,"Show more"}}
                    }
                } else if loading {
                    div {class:"canvas-skeleton",role:"status","aria-label":"Loading canvas",div {}div {}div {}div {}}
                }
            }
            }
        }
    }
}

fn render_node(
    node: &N,
    state: Signal<ShellState>,
    epoch: u64,
    checklist: &checklist::reader::Mapping,
    reader: Signal<Reader>,
    canonical: Option<&checklist::Presentation>,
    rename: Signal<Option<RenameDraft>>,
    text_draft: Signal<Option<text::Draft>>,
    parent_section: Option<&str>,
    comments: Signal<comments::State>,
    comments_open: Signal<bool>,
    local_review: Signal<Option<checklist::recovery::review::Draft>>,
) -> Element {
    let N::Element {
        kind,
        children,
        section,
    } = node
    else {
        let N::Text(text) = node else { unreachable!() };
        return rsx! {"{text}"};
    };
    let image_section = section.as_deref().or(parent_section);
    let content = || rsx! {for child in children {{render_node(child,state,epoch,checklist,reader,canonical,rename,text_draft,image_section,comments,comments_open,local_review)}}};
    match kind {
        K::Group => rsx! {div {{content()}}},
        K::Inline => rsx! {span {{content()}}},
        K::Paragraph => text::paragraph(
            state,
            reader,
            text_draft,
            section.as_deref(),
            epoch,
            rename,
            content(),
        ),
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
            let mapped = section.as_deref().and_then(|id| checklist.items.get(id));
            let checked = mapped.map_or(*checked, |item| item.checked);
            let current = canonical.filter(|view| checklist.owner == Some((view.visit, view.read)));
            let supported = mapped.is_some_and(|item| item.intent.is_some());
            let busy = reader.peek().loading
                || rename.peek().is_some()
                || text_draft.peek().is_some()
                || current.is_none_or(|view| {
                    view.loading
                        || view.stale
                        || view.operation.is_some()
                        || view.capacity_reason.is_some()
                });
            let reason = mapped
                .and_then(|item| item.reason.as_deref())
                .unwrap_or("This item could not be verified. Open in Slack to edit it.");
            let section_id = section.clone().unwrap_or_default();
            let (visit, read) = current.map_or((0, 0), |view| (view.visit, view.read));
            rsx! {li {class:"canvas-check-item","data-canvas-section":section.as_deref(),"data-canonical-mapped":mapped.is_some(),
                if supported {
                    button {class:if checked {"canvas-checkbox checked"}else{"canvas-checkbox"},type:"button",role:"checkbox",disabled:busy,"aria-checked":checked,"aria-label":if checked {"Mark checklist item incomplete"}else{"Mark checklist item complete"},title:if busy {"Wait for checklist state to refresh"}else{"Toggle checklist item"},onclick:move |_|{if rename.peek().is_none() && text_draft.peek().is_none() {checklist::save::toggle(state,reader,section_id.clone(),visit,read)}},if checked {{icon(Icon::Check,"icon")}}}
                } else {
                    span {class:if checked {"canvas-checkbox checked"}else{"canvas-checkbox"},role:"checkbox",tabindex:"0","aria-checked":checked,"aria-disabled":"true","aria-label":if checked {"Completed checklist item"}else{"Incomplete checklist item"},"aria-description":reason,title:reason,if checked {{icon(Icon::Check,"icon")}}}
                }
                div {{content()}}
            }}
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
        K::Image { url, alt } => comments::annotation::image::render(
            state,
            reader,
            comments,
            comments_open,
            canonical,
            image_section,
            url.as_deref(),
            alt,
            epoch,
            comments::annotation::image::Editors {
                rename,
                text: text_draft,
                review: local_review,
            },
        ),
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
    let disabled =
        value.busy || value.input.trim().is_empty() || !surface::visible(&state.read(), &target.id);
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
    if !surface::visible(&state.peek(), &target.id) {
        if let Some(d) = draft.write().as_mut() {
            d.error = Some("This tab is no longer available. Your edit is still here.".into());
        }
        return;
    }
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
        || !surface::visible(&state.peek(), &target.id)
        || state.peek().shortcut_generation != visit
        || !state
            .peek()
            .canvas
            .as_ref()
            .is_some_and(|c| c.id == target.id)
    {
        if ctx.current(&state.peek()) && surface::retains_local(&state.peek(), &target.id, visit) {
            if let Some(d) = draft.write().as_mut() {
                d.busy = false;
                d.error = Some("This tab changed while the name request was being checked. Your edit is kept here; cancel and review the Canvas before trying again.".into());
            }
        }
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
        || !surface::visible(&state.peek(), &target.id)
        || state.peek().shortcut_generation != visit
        || !state
            .peek()
            .canvas
            .as_ref()
            .is_some_and(|c| c.id == target.id)
    {
        if ctx.current(&state.peek()) && surface::retains_local(&state.peek(), &target.id, visit) {
            if let Some(d) = draft.write().as_mut() {
                d.busy = false;
                d.error = Some("This tab changed while the name request was being checked. Your edit is kept here; cancel and review the Canvas before trying again.".into());
            }
        }
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
                checklist::invalidate(&mut shell, &ctx.session.team_id, &target.id);
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

#[cfg(test)]
mod mapping_completion_tests {
    use super::*;

    #[test]
    fn same_owner_canonical_completion_reprojects_once() {
        let owner = Some((4, 9));
        let mut reader = Reader::default();
        assert!(reader.mapping_needed(owner, true));
        reader.checklist.owner = owner;
        reader.canonical_loading = true;
        assert!(!reader.mapping_needed(owner, true));
        assert!(reader.mapping_needed(owner, false));
        // Both the effect and a Reader-owned loader record the completed state.
        reader.canonical_loading = false;
        assert!(!reader.mapping_needed(owner, false));
        assert!(reader.mapping_needed(Some((4, 10)), true));
        assert!(reader.mapping_needed(None, false));
    }
}
