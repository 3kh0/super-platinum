use super::*;
pub(in crate::app_home::about::management) fn open(mut state: Signal<ShellState>, visit: u64) {
    let Some(mut o) =
        Original::capture(&state.peek(), visit).filter(|o| supported_first(&o.editor))
    else {
        return;
    };
    o.embedded = true;
    dioxus::core::spawn_forever(async move {
        let Some(release) = super::super::native::lock(&o).await else {
            return;
        };
        let id = o.id;
        {
            let mut s = state.write();
            if !o.current(&s, None) || !editable(&s, visit) {
                release.release();
                return;
            }
            let settings = settings::Settings::capture(&s, &o);
            let managers = managers::Managers::capture(&s, &o);
            let draft = Local::new(&o);
            s.app_home.about.management.permissions.confirmation = Some(Confirmation {
                selected: o.editor.copy_permissions()["permission_type"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                original: o,
                release,
                settings,
                managers,
                finish_up: Some(draft),
            });
        }
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let retired = {
                let s = state.peek();
                let Some(c) = s
                    .app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_ref()
                    .filter(|c| c.original.id == id)
                else {
                    break;
                };
                !c.original.current(&s, None)
            };
            if retired {
                state
                    .write()
                    .app_home
                    .about
                    .management
                    .permissions
                    .retire_confirmation();
                break;
            }
        }
    });
}
pub(super) fn bind(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id).map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !local(&state.peek(), visit, id).is_some_and(|(_, l)| l.expected() == e) {
            return;
        }
        let bound = native::bind(&o, &e).await;
        if !local(&state.peek(), visit, id).is_some_and(|(_, l)| l.expected() == e) {
            return;
        }
        {
            let mut s = state.write();
            let l = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .as_mut()
                .unwrap()
                .finish_up
                .as_mut()
                .unwrap();
            l.avatar.pane_bound = bound;
            if l.avatar.pane == avatar::Pane::Crop && !l.avatar.crop_bound {
                l.avatar.crop_bound = bound;
                if !bound {
                    l.avatar.notice = Some(
                        "The original crop controls could not be verified. No crop was sent."
                            .into(),
                    );
                }
            }
        }
        if bound && e["pane"] == "crop" {
            cropper::bind(state, &o, &e).await;
        }
        if bound && e["pane"] == "details" {
            super::super::bind(state, visit, id);
            settings::bind_settings(state, visit, id);
            managers::bind(state, visit, id);
        }
    });
}
pub(super) fn edit(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    kind: &'static str,
    value: String,
) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| l.avatar.pane == avatar::Pane::Details && l.avatar.pane_bound)
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, Some(&json!({"kind":kind,"value":value}))).await {
            return;
        }
        let mut s = state.write();
        if !local(&s, visit, id).is_some_and(|(_, l)| l.expected() == e) {
            return;
        }
        let l = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .finish_up
            .as_mut()
            .unwrap();
        match kind {
            "title" => l.title = value,
            "description" => l.description = value,
            "more" => l.more = true,
            _ => return,
        }
        l.epoch = next();
        drop(s);
        bind(state, visit, id);
    });
}
pub(in crate::app_home::about::management) fn cancel(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    dismiss(state, visit, id, "cancel");
}
pub(super) fn escape(state: Signal<ShellState>, visit: u64, id: u64) {
    dismiss(state, visit, id, "escape");
}
pub(super) fn upload_cancel(state: Signal<ShellState>, visit: u64, id: u64) {
    dismiss(state, visit, id, "upload-cancel");
}
fn dismiss(mut state: Signal<ShellState>, visit: u64, id: u64, kind: &'static str) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| {
                kind == "escape"
                    || l.avatar.pane_bound
                        && match kind {
                            "cancel" => l.avatar.pane == avatar::Pane::Details,
                            "upload-cancel" => l.avatar.pane == avatar::Pane::Upload,
                            _ => false,
                        }
            })
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, Some(&json!({"kind":kind}))).await {
            return;
        }
        let mut s = state.write();
        if !local(&s, visit, id).is_some_and(|(_, l)| l.expected() == e) {
            return;
        }
        // Cancel changes neither the original document nor its native values.
        s.app_home
            .about
            .management
            .permissions
            .retire_confirmation();
    });
}
pub(in crate::app_home::about::management) fn submit(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    let captured = {
        let s = state.peek();
        local(&s, visit, id)
            .filter(|(_, l)| l.valid())
            .map(|(c, l)| (c.original.clone(), l.expected()))
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !native::current(&o, &e, Some(&json!({"kind":"submit"}))).await
            || !managers::verify(state, &o).await
        {
            return;
        }
        let settings_expected = {
            let s = state.peek();
            local(&s, visit, id).and_then(|(c, _)| c.settings.as_ref().map(|v| v.expected()))
        };
        if let Some(e) = settings_expected {
            if !super::super::native::settings(&o, &e, None).await {
                return;
            }
        }
        let prepared = {
            let s = state.peek();
            let Some((c, l)) = local(&s, visit, id).filter(|(_, l)| l.expected() == e && l.valid())
            else {
                return;
            };
            let Some(m) = c
                .managers
                .as_ref()
                .filter(|m| m.valid() && m.current(&s, &o))
            else {
                return;
            };
            let Ok(manager) = m.draft.plan() else { return };
            let run = match c
                .settings
                .as_ref()
                .map(|v| v.desired_embedded())
                .transpose()
            {
                Ok(v) => v.flatten(),
                Err(_) => return,
            };
            let metadata =
                match o
                    .editor
                    .metadata_command(&l.title, &l.description, Some(l.icon_url.as_str()))
                {
                    Ok(cmd) => match MutationPlan::new(&o.editor, cmd) {
                        Ok(p) => Some(p),
                        Err(_) => return,
                    },
                    Err(_) => {
                        let w = o.editor.raw_workflow();
                        if w["title"].as_str() != Some(l.title.as_str())
                            || w["description"].as_str() != Some(l.description.as_str())
                            || w["icons"]["image_192"].as_str().unwrap_or_default() != l.icon_url
                        {
                            return;
                        }
                        None
                    }
                };
            let copy = if c.selected != o.editor.copy_permissions()["permission_type"] {
                MutationPlan::new(
                    &o.editor,
                    protocol::Command::CopyPermission(c.selected.clone()),
                )
                .ok()
            } else {
                None
            };
            let run = match run.map(|cmd| groups::Run::new(&o.editor, cmd)).transpose() {
                Ok(v) => v,
                Err(_) => return,
            };
            (metadata, copy, run, manager)
        };
        {
            let mut s = state.write();
            if !local(&s, visit, id).is_some_and(|(_, l)| l.expected() == e) {
                return;
            }
            let c = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .take()
                .unwrap();
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                s.app_home.about.management.permissions.confirmation = Some(c);
                return;
            };
            let Ok(publication) = MutationPlan::new(&o.editor, protocol::Command::Published(true))
            else {
                lease.finish(&mut s, None);
                c.release.release();
                return;
            };
            let (metadata, copy, run, manager) = prepared;
            let metadata_replies = metadata
                .as_ref()
                .map(|p| p.slots().iter().map(|_| None).collect())
                .unwrap_or_default();
            document_mut(&mut s, visit).unwrap().pending = Some(id);
            s.app_home
                .about
                .management
                .permissions
                .first_attempts
                .push(Attempt {
                    original: o,
                    lease,
                    local: c.finish_up.unwrap(),
                    settings: c.settings,
                    managers: c.managers,
                    metadata,
                    metadata_replies,
                    acknowledged: vec![],
                    copy,
                    copy_reply: None,
                    run,
                    manager: (!manager.slots().is_empty()).then(|| managers::Group::new(manager)),
                    publication,
                    publication_reply: None,
                    followup: None,
                    follow_reply: None,
                    validation: json!({"status":"pending"}),
                    phase: "validation",
                    continuation: None,
                    finished: false,
                    complete: false,
                    resolved: false,
                    closed: false,
                    dialog_original: None,
                    readback: None,
                    release: Some(c.release),
                    confirmation_add: None,
                    confirmation_share: None,
                });
        }
        execute::run(state, id).await;
    });
}
