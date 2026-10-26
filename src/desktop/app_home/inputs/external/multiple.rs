//! JQ84 keeps a visible widget array separate from the last emitted Home value.
use super::*;
use crate::app_home::inputs::{self, static_select::Binding};
use super_platinum_core::block_actions::form::{FormField, home::ExternalParent};
mod custody;
pub(super) use custody::{flush_pending, instance_current, parents, pending_revision};

pub(super) struct Widget {
    pub(super) visible: Vec<ExternalOption>,
    pub(super) accepted: Vec<ExternalOption>,
    pub(super) invalid: bool,
    pub(super) error: Option<String>,
    pub(super) revision: u64,
    initial: Option<Value>,
    mount: u64,
    instance: u64,
    parent: ExternalParent,
    binding: Binding,
    pending: Option<dispatch::Event>,
}
fn widget<'a>(s: &'a ShellState, key: &InputKey) -> Option<&'a Widget> {
    let index = (super::super::owner(s)?, key.clone());
    let c = s.app_home.external.catalogs.get(&index)?;
    crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
        .then_some(c.multi.as_ref())
        .flatten()
        .filter(|w| instance_current(s, &w.binding, w.instance))
}
pub(super) fn revision(s: &ShellState, key: &InputKey) -> u64 {
    widget(s, key).map_or(0, |w| w.revision)
}
pub(super) fn visible(s: &ShellState, key: &InputKey) -> Option<Vec<ExternalOption>> {
    widget(s, key).map(|w| w.visible.clone())
}
/// n9UX derives current rendering from props/count, independently of JQ84's
/// retained commit-suppression flag. A maximum-only update never emits a value.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Presentation {
    pub(super) maximum: Option<usize>,
    pub(super) count: usize,
    pub(super) parent_error: bool,
    pub(super) invalid: bool,
    pub(super) warning: bool,
    pub(super) text: Option<String>,
}
pub(super) fn presentation(s: &ShellState, key: &InputKey) -> Option<Presentation> {
    let w = widget(s, key)?;
    let field = super::super::form(s)?.fields.get(key)?;
    let InputKind::MultiExternalSelect {
        max_selected_items, ..
    } = field.kind
    else {
        return None;
    };
    let count = w.visible.len();
    let warning = max_selected_items.is_some_and(|max| count > max);
    let parent_error = super::super::presentation_error(s, key).is_some();
    Some(Presentation {
        maximum: max_selected_items,
        count,
        parent_error,
        invalid: parent_error || warning,
        warning,
        text: max_selected_items.map(|max| {
            if warning {
                format!("You have selected too many items, you can only go up to {max}.")
            } else {
                format!("You can select up to {max} items.")
            }
        }),
    })
}
fn validation(field: &FormField, count: usize) -> (bool, Option<String>) {
    let InputKind::MultiExternalSelect {
        max_selected_items, ..
    } = field.kind
    else {
        return (true, Some("This input changed.".into()));
    };
    if let Some(max) = max_selected_items.filter(|max| count > *max) {
        return (true, Some(format!("Select at most {max} options.")));
    }
    (
        false,
        (!field.optional && count == 0).then(|| "This field is required.".into()),
    )
}
/// Source prop reconciliation, independent of popup expansion and query results.
/// initial_options is the yIlz initialOptions prop; state is savedOptions and only
/// seeds a new mounted widget. A max-only update does not recompute isInvalid.
pub(crate) fn hydrate(s: &mut ShellState) {
    hydrate_initialized(s, &[]);
}
pub(super) fn hydrate_initialized(s: &mut ShellState, initialized: &[InputKey]) {
    let Some(owner) = super::super::owner(s) else {
        return;
    };
    let Some(h) = s.app_home.surface.as_ref().filter(|h| h.owner.current(s)) else {
        return;
    };
    let mount = h.id;
    let context = h.owner.ctx.clone();
    let fields = super::super::form(s)
        .map(|f| f.fields.clone())
        .unwrap_or_default();
    s.app_home
        .external
        .catalogs
        .retain(|(o, _), _| s.app_home.forms.contains_key(o));
    for (key, field) in fields {
        let InputValue::ExternalOptions(value) = &field.value else {
            continue;
        };
        if !matches!(field.kind, InputKind::MultiExternalSelect { .. }) {
            continue;
        }
        let Some(binding) = dispatch::binding(s, &key) else {
            continue;
        };
        let Some(form) = inputs::form(s) else {
            continue;
        };
        let seed = if initialized.contains(&key) {
            form.multi_external_initial_seed(&key)
        } else {
            form.multi_external_seed(&key)
        };
        let (Ok(seed), Ok(parent)) = (seed, form.multi_external_parent(&key)) else {
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
        let initial = field.raw.get("initial_options").cloned();
        // A new genuine Home visit constructs from the retained accepted form.
        // Popup/query/context changes within this visit keep the same widget.
        if catalog.multi.as_ref().is_none_or(|w| w.mount != mount) {
            // Retained dirty form data is not a replacement for lost source
            // custody. Remount only with the existing exact accepted proof.
            if field.dirty && !catalog.multi.as_ref().is_some_and(|w| &w.accepted == value) {
                continue;
            }
            catalog.multi = Some(Widget {
                visible: seed,
                accepted: value.clone(),
                invalid: false,
                error: field.error.clone(),
                revision: next(),
                initial,
                mount,
                instance: next(),
                parent,
                binding,
                pending: None,
            });
            continue;
        }
        let w = catalog.multi.as_mut().unwrap();
        // Initialized parent custody, not saved-only props, owns accepted data.
        if !field.dirty {
            w.accepted = value.clone();
        }
        if w.binding.raw != binding.raw || w.binding.visit != binding.visit {
            w.pending = None;
        }
        w.parent = parent;
        w.binding = binding;
        let changed_initial = w.initial != initial;
        w.initial = initial.clone();
        let options = changed_initial
            .then(|| {
                initial.as_ref().and_then(Value::as_array).and_then(|a| {
                    a.iter()
                        .map(ExternalOption::parse)
                        .collect::<Result<Vec<_>, _>>()
                        .ok()
                })
            })
            .flatten();
        if let Some(options) = options {
            let mut event = None;
            if apply(s, &key, options, &mut event) {
                if let Some(event) = event {
                    s.app_home
                        .external
                        .catalogs
                        .get_mut(&(owner.clone(), key.clone()))
                        .unwrap()
                        .multi
                        .as_mut()
                        .unwrap()
                        .pending = Some(event);
                }
            }
        }
    }
}

pub(super) fn toggle(
    s: &mut ShellState,
    ticket: Ticket,
    option: ExternalOption,
    event: &mut Option<dispatch::Event>,
) -> bool {
    let Some(mut options) = ticket.selected(s) else {
        return false;
    };
    if let Some(i) = options.iter().position(|old| old.value() == option.value()) {
        options.remove(i);
    } else {
        if options.len() >= 1000 {
            return false;
        }
        options.push(option);
    }
    commit(s, ticket, options, event)
}
pub(crate) fn remove(mut state: Signal<ShellState>, owner: QueryOwner, value: String) {
    let QueryOwner::Home(ticket) = owner else {
        return;
    };
    let mut event = None;
    remove_impl(&mut state.write(), ticket, &value, &mut event);
    dispatch::schedule(state, event);
}
pub(super) fn remove_local(s: &mut ShellState, ticket: Ticket, value: &str) -> bool {
    remove_impl(s, ticket, value, &mut None)
}
pub(super) fn remove_impl(
    s: &mut ShellState,
    ticket: Ticket,
    value: &str,
    event: &mut Option<dispatch::Event>,
) -> bool {
    let Some(mut options) = ticket.selected(s) else {
        return false;
    };
    let Some(i) = options.iter().position(|o| o.value() == value) else {
        return false;
    };
    options.remove(i);
    commit(s, ticket, options, event)
}
/// Rendered JQ84 selectedOptions lifecycle; equal arrays do not validate or emit.
fn apply(
    s: &mut ShellState,
    key: &InputKey,
    options: Vec<ExternalOption>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    let Some(w) = widget(s, key) else {
        return false;
    };
    if w.visible == options {
        return true;
    }
    if options.len() > 1000 || !inputs::editable(s) {
        return false;
    }
    let owner = w.binding.form.clone();
    let Some(field) = inputs::form(s).and_then(|f| f.fields.get(key)) else {
        return false;
    };
    let (invalid, error) = validation(field, options.len());
    let action = if invalid {
        None
    } else {
        let Some(form) = s.app_home.forms.get_mut(&owner) else {
            return false;
        };
        let Ok(action) = form.multi_external_input(key, options.clone()) else {
            return false;
        };
        inputs::bump(s, &owner);
        action
    };
    let parent = inputs::form(s).unwrap().multi_external_parent(key).unwrap();
    let w = s
        .app_home
        .external
        .catalogs
        .get_mut(&(owner, key.clone()))
        .unwrap()
        .multi
        .as_mut()
        .unwrap();
    w.visible = options;
    w.invalid = invalid;
    w.error = error;
    w.revision = next();
    w.parent = parent;
    if !invalid {
        w.accepted = w.visible.clone();
    }
    // A newer valid user callback supersedes an unconsumed prop action.
    // Invalid local/parent callbacks leave the older valid handoff intact.
    if action.is_some() {
        w.pending = None;
    }
    *event = action.map(|action| dispatch::Event {
        action,
        binding: w.binding.clone(),
        instance: w.instance,
    });
    if !invalid {
        let raw = w
            .visible
            .iter()
            .map(|o| o.raw().clone())
            .collect::<Vec<_>>();
        inputs::static_select::fixture::event(s, &serde_json::json!({"selectedOptions":raw}), key);
    }
    true
}
fn commit(
    s: &mut ShellState,
    ticket: Ticket,
    options: Vec<ExternalOption>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !ticket.current(s) || !apply(s, &ticket.key, options, event) {
        return false;
    }
    // Even a widget-only edit retires callbacks from its predecessor array.
    // Keep the actual prior rows and route cooldown; never strand loading=true.
    let Some(mut successor) = Ticket::capture(s, &ticket.key) else {
        return true;
    };
    if let Some(p) = s
        .app_home
        .external
        .picker
        .as_mut()
        .filter(|p| p.ticket.same_authority(&ticket))
    {
        successor.visit = p.ticket.visit;
        p.query.retire_request();
        p.ticket = successor;
    }
    true
}
pub(super) fn validated(
    s: &ShellState,
    owner: &FormOwner,
    key: &InputKey,
    field: &FormField,
) -> bool {
    if !field.dirty {
        return true;
    }
    let InputValue::ExternalOptions(options) = &field.value else {
        return false;
    };
    s.app_home
        .external
        .catalogs
        .get(&(owner.clone(), key.clone()))
        .is_some_and(|c| {
            crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                && c.multi.as_ref().is_some_and(|w| &w.accepted == options)
        })
}
pub(super) fn snapshot(s: &ShellState, key: &InputKey) -> Value {
    widget(s, key).map_or(Value::Null, |w| serde_json::json!({
        "visible":w.visible.iter().map(ExternalOption::raw).collect::<Vec<_>>(),
        "accepted":w.accepted.iter().map(ExternalOption::raw).collect::<Vec<_>>(),
        "invalid":w.invalid,"error":w.error,"revision":w.revision,"initial":w.initial,"mount":w.mount,"presentation":presentation(s,key),
        "instance":w.instance,"current":instance_current(s,&w.binding,w.instance),"loading":dispatch::disabled(s,key),
        "parent":inputs::form(s).and_then(|f|f.state().ok()).and_then(|v|v["values"][&key.block_id][&key.action_id].as_object().map(|o|Value::Object(o.clone()))),
        "pendingPropEvent":w.pending.is_some()
    }))
}

#[cfg(test)]
mod tests;
