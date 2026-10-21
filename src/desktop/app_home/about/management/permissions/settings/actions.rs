use super::*;

pub(in crate::app_home::about::management) fn change(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    action: Value,
) {
    let captured = {
        let s = state.peek();
        let Some(c) = owned(&s, visit, id) else {
            return;
        };
        let Some(settings) = c.settings.as_ref().filter(|v| v.current(&s, &c.original)) else {
            return;
        };
        {
            let Some(pane) = finish_up::details_action(c) else {
                return;
            };
            (
                c.original.clone(),
                settings.epoch,
                settings.expected(),
                pane,
            )
        }
    };
    dioxus::core::spawn_forever(async move {
        let (original, epoch, expected, pane) = captured;
        if !managers::verify(state, &original).await {
            return;
        }
        if !native::settings(&original, &expected, Some(&action)).await {
            return;
        }
        let mut s = state.write();
        let Some(c) = owned(&s, visit, id) else {
            return;
        };
        if !pane.current(c) {
            return;
        }
        if !c
            .settings
            .as_ref()
            .is_some_and(|v| v.epoch == epoch && v.current(&s, &original))
        {
            return;
        }
        let settings = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .settings
            .as_mut()
            .unwrap();
        let Some(draft) = settings.draft.as_mut() else {
            return;
        };
        let result = match action["kind"].as_str() {
            Some("mode") => Mode::parse(action["value"].as_str().unwrap_or_default())
                .and_then(|m| draft.select_mode(m, &settings.context)),
            Some("external") if settings.policy.external_checkbox_enabled() == Ok(true) => {
                draft.toggle_external(action["value"] == true, &settings.context)
            }
            Some("add") | Some("remove") => {
                let target = action["value"].as_str().unwrap_or_default();
                let remove = action["kind"] == "remove";
                let external = settings.context.is_external(target);
                if external.is_err()
                    || !remove
                        && !settings
                            .results
                            .iter()
                            .chain(&settings.external_results)
                            .any(|id| id == target)
                {
                    return;
                }
                let external = external.unwrap();
                if !remove && external && settings.policy.allows_external(target) != Ok(true) {
                    return;
                }
                let mut ids: Vec<_> = draft
                    .ids()
                    .iter()
                    .filter(|v| settings.context.is_external(v) == Ok(external))
                    .cloned()
                    .collect();
                if remove {
                    ids.retain(|v| v != target);
                } else if !ids.iter().any(|v| v == target) {
                    ids.push(target.into());
                }
                if external {
                    draft.replace_external(ids, &settings.context)
                } else {
                    draft.replace_internal(
                        ids.into_iter().map(Selection::Resolved).collect(),
                        &settings.context,
                    )
                }
            }
            _ => return,
        };
        let available_mode = if result == Err(domain::Error::MissingAvailable)
            && action["kind"] == "mode"
            && action["value"] == "everyone"
        {
            Some(Mode::Everyone)
        } else {
            None
        };
        let load_available = available_mode.is_some();
        if result.is_ok() || load_available {
            settings.epoch = next();
            settings.generation = next();
            settings.pending = load_available;
            settings.pending_mode = available_mode;
            settings.notice = None;
        } else {
            settings.notice =
                Some("These selections could not be verified. Your settings are unchanged.".into());
        }
        let generation = settings.generation;
        drop(s);
        bind_settings(state, visit, id);
        if load_available {
            acquire::search(state, original, generation, true, String::new()).await;
        } else {
            hydrate::start(state, visit, id, true);
        }
    });
}

pub(in crate::app_home::about::management) fn search(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    external: bool,
    text: String,
) {
    if text.len() > 1024 || text.chars().any(char::is_control) {
        return;
    }
    let captured = {
        let s = state.peek();
        let Some(c) = owned(&s, visit, id) else {
            return;
        };
        let Some(settings) = c.settings.as_ref().filter(|v| v.current(&s, &c.original)) else {
            return;
        };
        {
            let Some(pane) = finish_up::details_action(c) else {
                return;
            };
            (
                c.original.clone(),
                settings.epoch,
                settings.expected(),
                pane,
            )
        }
    };
    dioxus::core::spawn_forever(async move {
        let (original, epoch, expected, pane) = captured;
        let action = json!({"kind":if external{"external-query"}else{"query"},"value":text});
        if !managers::verify(state, &original).await {
            return;
        }
        if !native::settings(&original, &expected, Some(&action)).await {
            return;
        }
        let generation = {
            let mut s = state.write();
            let Some(c) = owned(&s, visit, id) else {
                return;
            };
            if !pane.current(c) {
                return;
            }
            if !c
                .settings
                .as_ref()
                .is_some_and(|v| v.epoch == epoch && v.current(&s, &original))
            {
                return;
            }
            let v = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .as_mut()
                .unwrap()
                .settings
                .as_mut()
                .unwrap();
            if external {
                v.external_query = text.clone();
                v.external_results.clear();
            } else {
                v.query = text.clone();
                v.results.clear();
            }
            v.epoch = next();
            v.generation = next();
            v.pending = true;
            v.notice = None;
            v.generation
        };
        tokio::time::sleep(Duration::from_millis(250)).await;
        acquire::search(state, original, generation, external, text).await;
    });
}

pub(in crate::app_home::about::management) fn bind_settings(
    state: Signal<ShellState>,
    visit: u64,
    id: u64,
) {
    let captured = {
        let s = state.peek();
        owned(&s, visit, id).and_then(|c| {
            c.settings
                .as_ref()
                .map(|v| (c.original.clone(), v.expected()))
        })
    };
    if let Some((original, expected)) = captured {
        dioxus::core::spawn_forever(async move {
            let current = {
                let s = state.peek();
                owned(&s, visit, id).is_some_and(|c| {
                    c.settings.as_ref().is_some_and(|v| {
                        Some(v.epoch) == expected["epoch"].as_u64() && v.current(&s, &original)
                    })
                })
            };
            if current {
                native::bind_settings(&original, &expected).await;
            }
        });
    }
}
