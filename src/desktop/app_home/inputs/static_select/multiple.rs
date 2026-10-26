//! Ordered Home input selection; parent state and mounted display have separate custody.
use super::*;
mod bridge;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
pub(crate) use bridge::Control;
pub(super) struct Widget {
    visit: u64,
    initial: Option<Value>,
    visible: Vec<Value>,
    invalid: bool,
}
pub(super) struct Mount {
    pub(super) id: u64,
    pub(super) seq: u64,
    pub(super) values: Vec<String>,
    pub(super) binding: Binding,
}
fn values(visible: &[Value]) -> Vec<String> {
    visible
        .iter()
        .filter_map(|o| o["value"].as_str().map(str::to_owned))
        .collect()
}
pub(crate) fn dispatching(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| {
        f.multi_static_field(key).is_ok() && f.dispatch(key) == Some(Dispatch::Change)
    })
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .iter()
        .filter(|(k, _)| dispatching(s, k))
        .map(|(k, f)| {
            let InputKind::StaticSelect { options, .. } = &f.kind else {
                unreachable!()
            };
            let visible = match &f.value {
                InputValue::Selected(v) => v
                    .iter()
                    .map(|v| {
                        options
                            .iter()
                            .find(|option| &option.value == v)
                            .map(|option| option.raw.clone())
                            .or_else(|| {
                                s.app_home
                                    .static_select
                                    .multiples
                                    .get(&(o.clone(), k.clone()))
                                    .and_then(|w| {
                                        w.visible
                                            .iter()
                                            .find(|option| {
                                                option["value"].as_str() == Some(v.as_str())
                                            })
                                            .cloned()
                                    })
                            })
                            .unwrap_or_else(
                                || json!({"value":v,"text":{"type":"plain_text","text":v}}),
                            )
                    })
                    .collect(),
                _ => vec![],
            };
            (k.clone(), f.raw.get("initial_options").cloned(), visible)
        })
        .collect::<Vec<_>>();
    s.app_home
        .static_select
        .multiples
        .retain(|(owner, key), _| {
            s.app_home.forms.contains_key(owner)
                && (owner != &o || fields.iter().any(|(k, _, _)| k == key))
        });
    for (key, initial, visible) in fields {
        let index = (o.clone(), key);
        if let Some(w) = s
            .app_home
            .static_select
            .multiples
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            if w.initial != initial {
                if let Some(v) = initial.as_ref().and_then(Value::as_array) {
                    w.visible = v.clone();
                }
                w.initial = initial;
            }
        } else if s.app_home.static_select.multiples.len() < 256
            || s.app_home.static_select.multiples.contains_key(&index)
        {
            s.app_home.static_select.multiples.insert(
                index,
                Widget {
                    visit,
                    initial,
                    visible,
                    invalid: false,
                },
            );
        }
    }
}
pub(super) fn ready(s: &ShellState) -> bool {
    let Some(f) = form(s) else { return false };
    let Some(o) = owner(s) else { return false };
    f.fields.keys().filter(|k| dispatching(s, k)).all(|k| {
        let i = (o.clone(), k.clone());
        s.app_home
            .static_select
            .multi_mounts
            .get(&i)
            .is_some_and(|m| {
                m.seq > 0
                    && m.binding.current(s)
                    && s.app_home
                        .static_select
                        .multiples
                        .get(&i)
                        .is_some_and(|w| values(&w.visible) == m.values)
            })
    })
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .app_home
        .static_select
        .multi_mounts
        .iter()
        .filter(|(_, m)| !m.binding.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let changed = !stale.is_empty();
    for k in stale {
        s.app_home.static_select.multi_mounts.remove(&k);
    }
    changed
}
#[derive(Clone, Serialize, Deserialize)]
struct Occurrence {
    mount: u64,
    seq: u64,
    kind: String,
    values: Vec<String>,
    trusted: bool,
}
fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<u64> {
    if !b.current(s) {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let field = form(s)?.multi_static_field(&b.key).ok()?;
    let InputKind::StaticSelect {
        options,
        max_selected_items,
        ..
    } = &field.kind
    else {
        return None;
    };
    let maximum = *max_selected_items;
    let mut seen = std::collections::BTreeSet::new();
    if e.values.len() > 1000 || e.values.iter().any(|v| !seen.insert(v)) {
        return None;
    }
    // Mounted/closed observations may retain display-only options removed by a
    // newer source. Only a current-option callback can mint selection authority.
    let selected = e
        .values
        .iter()
        .map(|v| {
            options
                .iter()
                .find(|o| &o.value == v)
                .map(|o| o.raw.clone())
        })
        .collect::<Option<Vec<_>>>();
    let m = s.app_home.static_select.multi_mounts.get_mut(&index)?;
    if m.id != e.mount || e.seq != m.seq + 1 {
        return None;
    }
    if e.kind != "change" && m.values != e.values {
        return None;
    }
    m.seq = e.seq;
    if e.kind != "change" || !b.interactive(s) {
        return None;
    }
    let selected = selected?;
    s.app_home
        .static_select
        .multi_mounts
        .get_mut(&index)?
        .values = e.values.clone();
    let w = s.app_home.static_select.multiples.get_mut(&index)?;
    w.visible = selected;
    w.invalid = maximum.is_some_and(|n| e.values.len() > n);
    let event = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .multi_static_input(&b.key, e.values.clone())
        .ok()?;
    bump(s, &b.form);
    super::fixture::event(s, e, &b.key);
    let event = event?;
    let route = context::Witness::capture(s).ok()?;
    let action = s
        .app_home
        .surface
        .as_ref()?
        .view
        .as_ref()?
        .static_action(&event, &b.owner.ctx.session.user_id)
        .ok()?;
    let id = next();
    let at = Instant::now();
    let q = Queued {
        id,
        at,
        binding: b.clone(),
        mount: e.mount,
        action,
        route,
        edge: "trailing",
    };
    s.app_home.static_select.windows.insert(
        index,
        Window {
            id,
            last: at,
            binding: b.clone(),
            trailing: Some(q),
        },
    );
    Some(id)
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let widgets=owner(s).map(|o|s.app_home.static_select.multiples.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|{
        let field=form(s).and_then(|f|f.multi_static_field(k).ok());
        let maximum=field.and_then(|f|match f.kind{InputKind::StaticSelect{max_selected_items,..}=>max_selected_items,_=>None});
        let error=super::super::presentation_error(s,k);
        json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"initial":w.initial,"visible":w.visible,"invalid":w.invalid,
            "presentation":{"maximum":maximum,"count":w.visible.len(),"parentError":error,"invalid":error.is_some()||maximum.is_some_and(|n|w.visible.len()>n),"text":maximum.map(|n|format!("You can select up to {n} items."))}})
    }).collect::<Vec<_>>());
    json!({"widgets":widgets,"mounts":s.app_home.static_select.multi_mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"values":m.values,"block":m.binding.key.block_id,"action":m.binding.key.action_id,"current":m.binding.current(s)})).collect::<Vec<_>>()})
}
