//! hGZA mounted local values. Every inline callback commits, even above max.
use super::*;

pub(super) struct Widget {
    pub(super) visit: u64,
    pub(super) instance: u64,
    pub(super) parent: super_platinum_core::block_actions::form::home::UserParent,
    pub(super) binding: super::super::static_select::Binding,
    pub(super) revision: u64,
    pub(super) initial: Option<Value>,
    pub(super) visible: Vec<String>,
    pub(super) retained_labels: BTreeMap<String, String>,
    pub(super) invalid: bool,
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(owner) = super::super::owner(s) else {
        return;
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return;
    };
    let visit = h.id;
    let fields: Vec<_> = super::super::form(s)
        .into_iter()
        .flat_map(|f| f.fields.iter())
        .filter(|(_, f)| matches!(f.kind, InputKind::MultiUser { .. }))
        .filter_map(|(k, field)| {
            let f = super::super::form(s)?;
            Some((
                k.clone(),
                field.raw.get("initial_users").cloned(),
                f.multi_user_seed(k).ok()?,
                f.multi_user_parent(k).ok()?,
                dispatch::binding(s, k)?,
            ))
        })
        .collect();
    s.app_home.users.multiples.retain(|(o, k), _| {
        o.epoch == s.core.account_epoch
            && s.app_home.forms.contains_key(o)
            && (o != &owner || fields.iter().any(|(key, _, _, _, _)| key == k))
    });
    for (key, initial, visible, parent, binding) in fields {
        let index = (owner.clone(), key);
        if let Some(w) = s
            .app_home
            .users
            .multiples
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            w.parent = parent;
            w.binding = binding;
            if w.initial != initial {
                // Null/absent initialUsers is falsey; [] is truthy. No emission
                // or internal validity recomputation occurs in this prop effect.
                if let Some(ids) = initial.as_ref().and_then(Value::as_array) {
                    w.visible = ids
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    w.retained_labels.retain(|id, _| w.visible.contains(id));
                }
                w.initial = initial;
                w.revision = next();
            }
        } else {
            if s.app_home.users.multiples.len() >= 256
                && !s.app_home.users.multiples.contains_key(&index)
            {
                continue;
            }
            s.app_home.users.multiples.insert(
                index,
                Widget {
                    visit,
                    instance: next(),
                    parent,
                    binding,
                    revision: next(),
                    initial,
                    visible,
                    retained_labels: BTreeMap::new(),
                    invalid: false,
                },
            );
        }
    }
}
pub(super) fn display(
    s: &ShellState,
    owner: &FormOwner,
    key: &InputKey,
    visit: u64,
    raw: &Value,
) -> Option<Vec<(String, String)>> {
    let w = s
        .app_home
        .users
        .multiples
        .get(&(owner.clone(), key.clone()))?;
    if w.visit != visit || w.initial != raw.get("initial_users").cloned() {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    let workspace = s.core.workspaces.get(&h.owner.ctx.session.team_id)?;
    Some(
        w.visible
            .iter()
            .map(|id| {
                let label = workspace
                    .users
                    .get(id)
                    .map(|_| workspace.display_name(id))
                    .or_else(|| w.retained_labels.get(id).cloned())
                    .unwrap_or_else(|| id.clone());
                (id.clone(), label)
            })
            .collect(),
    )
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Presentation {
    pub(crate) maximum: Option<usize>,
    pub(crate) count: usize,
    pub(crate) parent_error: Option<String>,
    pub(crate) invalid: bool,
    pub(crate) text: Option<String>,
}
pub(crate) fn presentation(s: &ShellState, key: &InputKey) -> Option<Presentation> {
    let displayed = super::display(s, key)?;
    let field = super::super::form(s)?.fields.get(key)?;
    let InputKind::MultiUser { max_selected_items } = field.kind else {
        return None;
    };
    let count = displayed.len();
    let parent_error = super::super::presentation_error(s, key);
    Some(Presentation {
        maximum: max_selected_items,
        count,
        invalid: parent_error.is_some() || max_selected_items.is_some_and(|max| count > max),
        parent_error,
        text: max_selected_items.map(|max| format!("You can select up to {max} people.")),
    })
}
pub(super) fn toggle(
    s: &mut ShellState,
    t: &Ticket,
    id: &str,
    label: String,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) {
        return false;
    }
    let Some(w) = s
        .app_home
        .users
        .multiples
        .get(&(t.form.clone(), t.key.clone()))
    else {
        return false;
    };
    let mut values = w.visible.clone();
    if values.iter().any(|v| v == id) {
        values.retain(|v| v != id)
    } else {
        if values.len() >= 1000 {
            return false;
        }
        values.push(id.into());
    }
    edit(s, t, values, Some((id.into(), label)), event)
}
pub(super) fn remove(
    s: &mut ShellState,
    t: &Ticket,
    id: &str,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) {
        return false;
    }
    let Some(w) = s
        .app_home
        .users
        .multiples
        .get(&(t.form.clone(), t.key.clone()))
    else {
        return false;
    };
    if !w.visible.iter().any(|v| v == id) {
        return false;
    }
    let values = w
        .visible
        .iter()
        .filter(|v| v.as_str() != id)
        .cloned()
        .collect();
    edit(s, t, values, None, event)
}
fn edit(
    s: &mut ShellState,
    t: &Ticket,
    values: Vec<String>,
    label: Option<(String, String)>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) || !super::super::editable(s) {
        return false;
    }
    let Some(field) = super::super::form(s).and_then(|f| f.fields.get(&t.key)) else {
        return false;
    };
    let InputKind::MultiUser { max_selected_items } = field.kind else {
        return false;
    };
    let invalid = max_selected_items.is_some_and(|max| max > 0 && values.len() > max);
    let Some(form) = s.app_home.forms.get_mut(&t.form) else {
        return false;
    };
    let Ok(action) = form.multi_user_input(&t.key, values.clone()) else {
        return false;
    };
    let Ok(parent) = form.multi_user_parent(&t.key) else {
        return false;
    };
    super::super::bump(s, &t.form);
    let w = s
        .app_home
        .users
        .multiples
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    w.parent = parent;
    *event = action.map(|action| dispatch::Event {
        action,
        binding: w.binding.clone(),
        instance: w.instance,
    });
    w.visible = values.clone();
    w.invalid = invalid;
    w.revision = next();
    if let Some((id, label)) = label {
        w.retained_labels.insert(id, label);
    }
    w.retained_labels.retain(|id, _| w.visible.contains(id));
    // The old callback/request lease is spent by the form update. Keep the
    // current rows and query in the same picker, but retire its pending read.
    let now = t.after_edit(s);
    if let Some(p) = s
        .app_home
        .users
        .picker
        .as_mut()
        .filter(|p| p.ticket.same(t))
    {
        p.cancel();
        p.loading = false;
        p.query_revision = next();
        if let Some(now) = now {
            p.ticket = now
        } else {
            s.app_home.users.picker = None;
        }
    }
    super::super::static_select::fixture::event(s, &json!({"selectedUsers":values}), &t.key);
    true
}
#[cfg(test)]
mod tests;
