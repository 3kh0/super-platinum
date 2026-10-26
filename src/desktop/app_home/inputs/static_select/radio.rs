//! Home radio mounts retain a visible value independently of committed source metadata.
use super::*;
mod bridge;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
pub(crate) use bridge::Control;

pub(super) struct Widget {
    visit: u64,
    visible: Option<String>,
    validated: bool,
}
pub(super) struct Mount {
    pub(super) id: u64,
    pub(super) seq: u64,
    pub(super) value: Option<String>,
    pub(super) binding: Binding,
}
pub(crate) fn supported(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.radio_field(key).is_ok())
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .keys()
        .filter_map(|k| f.radio_seed(k).ok().map(|v| (k.clone(), v)))
        .collect::<Vec<_>>();
    s.app_home.static_select.radios.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _)| k == key))
    });
    for (key, visible) in fields {
        let index = (o.clone(), key);
        // gMsa has no prop update synchronization. Seed once per Home visit
        // using committed nonempty value || source initial value (empty allowed).
        if s.app_home
            .static_select
            .radios
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            continue;
        }
        if s.app_home.static_select.radios.len() < 256
            || s.app_home.static_select.radios.contains_key(&index)
        {
            s.app_home.static_select.radios.insert(
                index,
                Widget {
                    visit,
                    visible,
                    validated: false,
                },
            );
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
            .radio_mounts
            .get(&index)
            .is_some_and(|m| {
                m.seq > 0
                    && m.binding.current(s)
                    && s.app_home
                        .static_select
                        .radios
                        .get(&index)
                        .is_some_and(|w| w.visible == m.value)
            })
    })
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .app_home
        .static_select
        .radio_mounts
        .iter()
        .filter(|(_, m)| !m.binding.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let changed = !stale.is_empty();
    for k in stale {
        s.app_home.static_select.radio_mounts.remove(&k);
    }
    changed
}
#[derive(Clone, Serialize, Deserialize)]
struct Occurrence {
    mount: u64,
    seq: u64,
    kind: String,
    index: Option<usize>,
    value: Option<String>,
    trusted: bool,
}
fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<(u64, Option<Queued>)> {
    if !b.current(s) {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let m = s.app_home.static_select.radio_mounts.get(&index)?;
    if m.id != e.mount || e.seq != m.seq + 1 {
        return None;
    }
    if e.kind != "change" {
        if !matches!(e.kind.as_str(), "mounted" | "closed")
            || m.value != e.value
            || e.index.is_some()
        {
            return None;
        }
        s.app_home.static_select.radio_mounts.get_mut(&index)?.seq = e.seq;
        return None;
    }
    // The native listener admits the occurrence before publishing it. A prior
    // callback can start loading while this already-published event is in flight.
    if !b.interactive(s) {
        return None;
    }
    let field = form(s)?.radio_field(&b.key).ok()?;
    let InputKind::Radio { options } = &field.kind else {
        return None;
    };
    let value = options.get(e.index?)?.value.clone();
    if e.value.as_deref() != Some(value.as_str()) || m.value.as_deref() == Some(value.as_str()) {
        return None;
    }
    let mut next_form = form(s)?.clone();
    let event = next_form.radio_input(&b.key, e.index).ok()?;
    // Native radio occurrences always name a current row; no clear UI exists.
    s.app_home.forms.insert(b.form.clone(), next_form);
    let widget = s.app_home.static_select.radios.get_mut(&index)?;
    widget.visible = Some(value.clone());
    widget.validated = true;
    let m = s.app_home.static_select.radio_mounts.get_mut(&index)?;
    m.seq = e.seq;
    m.value = Some(value);
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
/// The callback diagnostic does not supply Home's absent parent-error context.
pub(crate) fn presentation_error(s: &ShellState, key: &InputKey) -> Option<String> {
    super::super::presentation_error(s, key)
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let widgets=owner(s).map(|o|s.app_home.static_select.radios.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"visible":w.visible,"validated":w.validated})).collect::<Vec<_>>());
    json!({"widgets":widgets,"mounts":s.app_home.static_select.radio_mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"value":m.value,"block":m.binding.key.block_id,"action":m.binding.key.action_id,"current":m.binding.current(s)})).collect::<Vec<_>>()})
}
