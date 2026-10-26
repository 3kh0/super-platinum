//! Home checkbox mounts retain ordered, source-owned raw survivors.
use super::*;
use super_platinum_core::block_actions::form::home::CheckboxSelection;
mod bridge;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
pub(crate) use bridge::Control;

pub(super) struct Widget {
    visit: u64,
    selection: CheckboxSelection,
}
pub(super) struct Mount {
    pub(super) id: u64,
    pub(super) seq: u64,
    pub(super) values: Vec<String>,
    pub(super) binding: Binding,
}
fn values(selection: &CheckboxSelection) -> Vec<String> {
    selection
        .selected()
        .iter()
        .filter_map(|v| v["value"].as_str().map(str::to_owned))
        .collect()
}
pub(crate) fn supported(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.checkbox_field(key).is_ok())
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .keys()
        .filter_map(|k| f.checkbox_selection(k).ok().map(|v| (k.clone(), v)))
        .collect::<Vec<_>>();
    s.app_home.static_select.checks.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _)| k == key))
    });
    for (key, selection) in fields {
        let index = (o.clone(), key);
        // s+nw has no prop update synchronization. A new Home visit seeds from
        // the current parent; source refresh inside this visit keeps survivors.
        if s.app_home
            .static_select
            .checks
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            continue;
        }
        if s.app_home.static_select.checks.len() < 256
            || s.app_home.static_select.checks.contains_key(&index)
        {
            s.app_home
                .static_select
                .checks
                .insert(index, Widget { visit, selection });
        }
    }
}
pub(super) fn ready(s: &ShellState) -> bool {
    let Some(f) = form(s) else { return false };
    let Some(o) = owner(s) else { return false };
    f.fields.keys().filter(|k| supported(s, k)).all(|k| {
        let index = (o.clone(), k.clone());
        s.app_home
            .static_select
            .check_mounts
            .get(&index)
            .is_some_and(|m| {
                m.seq > 0
                    && m.binding.current(s)
                    && s.app_home
                        .static_select
                        .checks
                        .get(&index)
                        .is_some_and(|w| values(&w.selection) == m.values)
            })
    })
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .app_home
        .static_select
        .check_mounts
        .iter()
        .filter(|(_, m)| !m.binding.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let changed = !stale.is_empty();
    for k in stale {
        s.app_home.static_select.check_mounts.remove(&k);
    }
    changed
}
#[derive(Clone, Serialize, Deserialize)]
struct Occurrence {
    mount: u64,
    seq: u64,
    kind: String,
    index: Option<usize>,
    checked: Option<bool>,
    values: Vec<String>,
    trusted: bool,
}
fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<(u64, Option<Queued>)> {
    if !b.current(s) {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let m = s.app_home.static_select.check_mounts.get(&index)?;
    if m.id != e.mount || e.seq != m.seq + 1 {
        return None;
    }
    if e.kind != "change" {
        if !matches!(e.kind.as_str(), "mounted" | "closed")
            || m.values != e.values
            || e.index.is_some()
            || e.checked.is_some()
        {
            return None;
        }
        s.app_home.static_select.check_mounts.get_mut(&index)?.seq = e.seq;
        return None;
    }
    // The native listener admits the occurrence before publishing it. A prior
    // callback can start loading while this already-published event is in flight.
    if !b.interactive(s) {
        return None;
    }
    let mut selection = s
        .app_home
        .static_select
        .checks
        .get(&index)?
        .selection
        .clone();
    let mut next_form = form(s)?.clone();
    let event = next_form
        .checkbox_input(&b.key, &mut selection, e.index?, e.checked?)
        .ok()?;
    if values(&selection) != e.values {
        return None;
    }
    // Commit only after the exact occurrence agrees with sealed core custody.
    s.app_home.forms.insert(b.form.clone(), next_form);
    s.app_home.static_select.checks.get_mut(&index)?.selection = selection;
    let m = s.app_home.static_select.check_mounts.get_mut(&index)?;
    m.seq = e.seq;
    m.values = e.values.clone();
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
    let mut q = Queued {
        id,
        at,
        binding: b.clone(),
        mount: e.mount,
        action,
        route,
        edge: "leading",
    };
    if let Some(w) = s.app_home.static_select.windows.get_mut(&index) {
        w.id = id;
        w.last = at;
        q.edge = "trailing";
        w.trailing = Some(q);
        Some((id, None))
    } else {
        s.app_home.static_select.windows.insert(
            index,
            Window {
                id,
                last: at,
                binding: b.clone(),
                trailing: None,
            },
        );
        Some((id, Some(q)))
    }
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let widgets=owner(s).map(|o|s.app_home.static_select.checks.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"visible":w.selection.selected()})).collect::<Vec<_>>());
    json!({"widgets":widgets,"mounts":s.app_home.static_select.check_mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"values":m.values,"block":m.binding.key.block_id,"action":m.binding.key.action_id,"current":m.binding.current(s)})).collect::<Vec<_>>()})
}
