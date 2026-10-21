use super::*;
pub(in crate::app_home::about::management::permissions) fn bind(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    let captured = {
        let s = state.peek();
        settings::owned(&s, visit, id).and_then(|c| {
            c.managers
                .as_ref()
                .map(|m| (c.original.clone(), m.expected()))
        })
    };
    if let Some((o, e)) = captured {
        dioxus::core::spawn_forever(async move {
            if native::witness(&o, &e, None, true).await {
                acquire::start(state, visit, id, true, false);
            }
        });
    }
}
pub(super) async fn checked(
    state: Signal<ShellState>,
    o: &Original,
    e: &Value,
    action: Option<&Value>,
) -> bool {
    let run = {
        let s = state.peek();
        let Some(c) = settings::owned(&s, o.visit, o.id) else {
            return false;
        };
        c.settings.as_ref().map(|v| v.expected())
    };
    if let Some(run) = run {
        if !super::super::native::settings(o, &run, None).await {
            return false;
        }
    }
    native::witness(o, e, action, false).await
        && owned(&state.peek(), o)
            .is_some_and(|m| m.current(&state.peek(), o) && Some(m.epoch) == e["epoch"].as_u64())
}
pub(in crate::app_home::about::management::permissions) fn change(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
    kind: &'static str,
    value: String,
) {
    edit(state, visit, id, json!({"kind":kind,"value":value}));
}
pub(in crate::app_home::about::management::permissions) fn search(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
    value: String,
) {
    if value.len() <= 1024 && !value.chars().any(char::is_control) {
        edit(state, visit, id, json!({"kind":"query","value":value}));
    }
}
pub(in crate::app_home::about::management::permissions) fn bump_cancel(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    edit(state, visit, id, json!({"kind":"bump-cancel"}));
}
fn edit(mut state: Signal<ShellState>, visit: u64, id: u64, action: Value) {
    let captured = {
        let s = state.peek();
        settings::owned(&s, visit, id).and_then(|c| {
            c.managers
                .as_ref()
                .filter(|m| m.current(&s, &c.original))
                .map(|m| (c.original.clone(), m.expected()))
        })
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !checked(state, &o, &e, Some(&action)).await {
            return;
        }
        let mut s = state.write();
        let Some(c) = settings::owned(&s, visit, id) else {
            return;
        };
        if !c
            .managers
            .as_ref()
            .is_some_and(|m| Some(m.epoch) == e["epoch"].as_u64() && m.current(&s, &o))
        {
            return;
        }
        let c = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap();
        let m = c.managers.as_mut().unwrap();
        let query = action["kind"] == "query";
        // Only a genuine original-query edit can supersede held app facts.
        // Ordinary directory loading and all selection actions remain blocked.
        if m.pending && !(query && m.facts_loading())
            || m.speed_bump && action["kind"] != "bump-cancel"
        {
            return;
        }
        match action["kind"].as_str() {
            Some("bump-cancel") => m.speed_bump = false,
            Some("query") => {
                m.query = action["value"].as_str().unwrap_or_default().into();
                m.results.clear();
                m.agent_facts = None;
                m.pending = false;
            }
            Some("add") | Some("remove") => {
                let target = action["value"].as_str().unwrap_or_default();
                let mut ids = m.ids();
                if action["kind"] == "add" {
                    if !m.results.iter().any(|v| v == target)
                        || !m.rows.get(target).is_some_and(|r| r.eligible)
                    {
                        return;
                    }
                    if !ids.iter().any(|v| v == target) {
                        ids.push(target.into());
                    }
                } else {
                    ids.retain(|v| v != target);
                }
                m.draft.select(ids.iter().cloned().map(Some).collect());
                if let Some(v) = c.settings.as_mut() {
                    v.context.collaborators = ids.clone();
                    for row in m.rows.values() {
                        if let Some((entity, r)) = source::run_row(row, &ids, &v.context) {
                            v.context.entities.retain(|e| e.id != entity.id);
                            v.context.entities.push(entity);
                            v.rows.insert(r.id.clone(), r);
                        }
                    }
                    if let Some(d) = v
                        .draft
                        .as_mut()
                        .filter(|d| d.mode() == protocol::run_settings::Mode::NamedEntities)
                    {
                        // Manager changes use the source's manager-first Set order;
                        // ordinary internal-picker updates intentionally preserve prior order.
                        if d.union_managers(&ids, &v.context).is_err() {
                            m.notice =
                                Some("Resolve the run-access selection before saving.".into());
                        }
                    }
                    v.epoch = next();
                    v.generation = next();
                }
            }
            _ => return,
        }
        m.epoch = next();
        m.generation = next();
        let generation = m.generation;
        drop(s);
        bind(state, visit, id);
        settings::bind_settings(state, visit, id);
        if query {
            acquire::start_search(state, o, generation).await;
        }
    });
}
