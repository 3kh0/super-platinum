//! Single external options separate full parent provenance from mapped display.
use super::*;
use crate::app_home::inputs::{self, static_select::Binding};
use serde_json::json;
use super_platinum_core::block_actions::form::home::{ExternalParent, StaticEvent};
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

pub(super) struct Widget {
    visit: u64,
    instance: u64,
    revision: u64,
    initial: Option<Value>,
    visible: Option<ExternalOption>,
    parent: ExternalParent,
    binding: Binding,
}
pub(super) struct Event {
    pub(super) action: StaticEvent,
    pub(super) binding: Binding,
    pub(super) instance: u64,
}
pub(super) fn binding(s: &ShellState, key: &InputKey) -> Option<Binding> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let f = inputs::form(s)?;
    if !matches!(
        f.external_field(key).ok()?.kind,
        InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
    ) {
        return None;
    }
    Some(Binding {
        owner: h.owner.clone(),
        form: inputs::owner(s)?,
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key.clone(),
        raw: f.raw_view().clone(),
    })
}
fn widget<'a>(s: &'a ShellState, key: &InputKey) -> Option<&'a Widget> {
    let c = s
        .app_home
        .external
        .catalogs
        .get(&(inputs::owner(s)?, key.clone()))?;
    if !crate::message_buttons::inputs::user::directory::context_current(s, &c.context) {
        return None;
    }
    let w = c.single.as_ref()?;
    instance_current(s, &w.binding, w.instance).then_some(w)
}
/// Outer None means unavailable; inner None is a current empty display.
pub(crate) fn display(s: &ShellState, key: &InputKey) -> Option<Option<ExternalOption>> {
    Some(widget(s, key)?.visible.clone())
}
pub(super) fn revision(s: &ShellState, key: &InputKey) -> Option<u64> {
    Some(widget(s, key)?.revision)
}
pub(crate) fn instance_current(s: &ShellState, b: &Binding, instance: u64) -> bool {
    if multiple::instance_current(s, b, instance) {
        return true;
    }
    b.current(s)
        && inputs::form(s).is_some_and(|f| {
            f.external_field(&b.key)
                .is_ok_and(|f| matches!(f.kind, InputKind::ExternalSelect { .. }))
        })
        && s.app_home
            .external
            .catalogs
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|c| {
                crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                    && c.single
                        .as_ref()
                        .is_some_and(|w| w.visit == b.visit.visit && w.instance == instance)
            })
}
pub(crate) fn disabled(s: &ShellState, key: &InputKey) -> bool {
    inputs::form(s).is_some_and(|f| {
        f.external_field(key).is_ok_and(|f| {
            matches!(
                f.kind,
                InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
            )
        })
    }) && inputs::static_select::loading(s, &key.block_id)
}
pub(crate) fn parents(s: &ShellState) -> Vec<ExternalParent> {
    let Some(owner) = inputs::owner(s) else {
        return vec![];
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return vec![];
    };
    let mut result: Vec<_> = s
        .app_home
        .external
        .catalogs
        .iter()
        .filter_map(|((o, key), c)| {
            let w = c.single.as_ref()?;
            // A removed/retyped field has no surviving initialized single widget.
            // Generic reconciliation still enforces its dirty-state safeguards.
            let survives = h.view.as_ref().is_some_and(|v| {
                v.raw()["blocks"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|b| {
                        b["type"] == "input"
                            && b["block_id"] == key.block_id
                            && b["element"]["action_id"] == key.action_id
                            && b["element"]["type"] == "external_select"
                    })
                })
            });
            (survives
                && o == &owner
                && w.visit == h.id
                && w.binding.owner.current(s)
                && crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                && inputs::form(s).is_some_and(|f| f.raw_view() == &w.binding.raw))
            .then(|| w.parent.clone())
        })
        .collect();
    result.extend(multiple::parents(s));
    result
}
/// Only successful form initialization/reconciliation supplies newly introduced
/// parent keys; absence of a catalog alone is never initial-parent authority.
pub(super) fn hydrate(s: &mut ShellState, initialized: &[InputKey]) {
    let Some(owner) = inputs::owner(s) else {
        return;
    };
    let Some(h) = s.app_home.surface.as_ref().filter(|h| h.owner.current(s)) else {
        return;
    };
    let visit = h.id;
    let context = h.owner.ctx.clone();
    let fields = inputs::form(s)
        .map(|f| f.fields.clone())
        .unwrap_or_default();
    s.app_home
        .external
        .catalogs
        .retain(|(o, _), _| s.app_home.forms.contains_key(o));
    for ((o, key), catalog) in &mut s.app_home.external.catalogs {
        if o == &owner {
            let kind = fields.get(key).map(|f| &f.kind);
            if !matches!(kind, Some(InputKind::ExternalSelect { .. })) {
                catalog.single = None;
            }
            if !matches!(kind, Some(InputKind::MultiExternalSelect { .. })) {
                catalog.multi = None;
            }
        }
    }
    for (key, field) in fields {
        let InputValue::External(value) = &field.value else {
            continue;
        };
        let Some(b) = binding(s, &key) else { continue };
        let Some(f) = inputs::form(s) else { continue };
        let seed = if initialized.contains(&key) {
            f.external_initial_seed(&key)
        } else {
            f.external_seed(&key)
        };
        let (Ok(seed), Ok(parent)) = (seed, f.external_parent(&key)) else {
            continue;
        };
        let index = (owner.clone(), key.clone());
        let stale = s.app_home.external.catalogs.get(&index).is_some_and(|c| {
            !crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
        });
        if stale {
            s.app_home.external.catalogs.remove(&index);
        }
        let catalog = s
            .app_home
            .external
            .catalogs
            .entry(index)
            .or_insert_with(|| Catalog {
                context: context.clone(),
                rows: vec![],
                selected: None,
                single: None,
                multi: None,
            });
        // A dirty descriptor is not self-authenticating after provenance is lost.
        if field.dirty && value.is_some() && &catalog.selected != value {
            continue;
        }
        if !field.dirty {
            catalog.selected = value.clone();
        }
        let initial = field.raw.get("initial_option").cloned();
        if catalog.single.as_ref().is_none_or(|w| w.visit != visit) {
            catalog.single = Some(Widget {
                visit,
                instance: next(),
                revision: next(),
                initial,
                visible: seed,
                parent,
                binding: b,
            });
            continue;
        }
        let w = catalog.single.as_mut().unwrap();
        // IKXd uses deep equality. A source-only display change never changes
        // the full parent descriptor or upgrades current query row authority.
        if w.initial != initial {
            w.visible = initial
                .as_ref()
                .filter(|v| !v.is_null())
                .and_then(|v| ExternalOption::parse(v).ok());
            w.revision = next();
        }
        w.initial = initial;
        w.parent = parent;
        w.binding = b;
    }
}
pub(super) fn edit(
    s: &mut ShellState,
    t: &Ticket,
    selected: Option<ExternalOption>,
    event: &mut Option<Event>,
) -> bool {
    if !t.current(s) || !inputs::editable(s) {
        return false;
    }
    let Some(form) = s.app_home.forms.get_mut(&t.form) else {
        return false;
    };
    let Ok(action) = form.external_input(&t.key, selected.clone()) else {
        return false;
    };
    let Ok(parent) = form.external_parent(&t.key) else {
        return false;
    };
    inputs::bump(s, &t.form);
    let catalog = s
        .app_home
        .external
        .catalogs
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    let w = catalog.single.as_mut().unwrap();
    w.parent = parent;
    w.visible = selected.clone();
    w.revision = next();
    catalog.selected = selected.clone();
    *event = action.map(|action| Event {
        action,
        binding: w.binding.clone(),
        instance: w.instance,
    });
    s.app_home.external.picker = None;
    inputs::static_select::fixture::event(
        s,
        &json!({"selectedOption":selected.as_ref().map(ExternalOption::raw)}),
        &t.key,
    );
    true
}
pub(super) fn schedule(state: Signal<ShellState>, event: Option<Event>) {
    if let Some(e) = event {
        inputs::static_select::typed_event(state, e.binding, e.instance, e.action);
    }
}
/// Product Reset clears only this form owner's single/multiple option custody.
pub(in crate::app_home::inputs) fn reset(s: &mut ShellState, owner: &FormOwner) {
    if s.app_home
        .external
        .picker
        .as_ref()
        .is_some_and(|p| &p.ticket.form == owner)
    {
        s.app_home.external.picker = None; // Query drop cancels its held read.
    }
    s.app_home.external.catalogs.retain(|(o, _), _| o != owner);
}
/// Logical widget/candidate proof only; no synthetic native option registry.
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(owner) = inputs::owner(s) else {
        return json!({"externalWidgets":null,"externalPickerBlock":null,"externalCatalogs":null,"multiExternalWidgets":null});
    };
    let state = inputs::form(s).and_then(|f| f.state().ok());
    json!({
        "externalWidgets":s.app_home.external.catalogs.iter().filter(|((o,_),_)|o==&owner)
            .filter_map(|((_,k),c)|c.single.as_ref().map(|w|json!({
                "block":k.block_id,"action":k.action_id,"visit":w.visit,"instance":w.instance,
                "revision":w.revision,"initial":w.initial,"visible":w.visible.as_ref().map(ExternalOption::raw),
                "parent":state.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&k.block_id)).and_then(|v|v.get(&k.action_id)),
                "accepted":c.selected.as_ref().map(ExternalOption::raw),
                "current":instance_current(s,&w.binding,w.instance),"loading":disabled(s,k)
            }))).collect::<Vec<_>>(),
        "externalPickerBlock":s.app_home.external.picker.as_ref().filter(|p|p.ticket.form==owner).map(|p|&p.ticket.key.block_id),
        "externalCatalogs":s.app_home.external.catalogs.iter().filter(|((o,_),_)|o==&owner)
            .map(|((_,k),c)|json!({"block":k.block_id,"action":k.action_id,
                "rows":c.rows.iter().map(ExternalOption::raw).collect::<Vec<_>>(),
                "selected":c.selected.as_ref().map(ExternalOption::raw),
                "current":crate::message_buttons::inputs::user::directory::context_current(s,&c.context)})).collect::<Vec<_>>(),
        "multiExternalWidgets":s.app_home.external.catalogs.iter().filter(|((o,_),c)|o==&owner&&c.multi.is_some())
            .map(|((_,k),_)|json!({"block":k.block_id,"action":k.action_id,"widget":multiple::snapshot(s,k)})).collect::<Vec<_>>()
    })
}
