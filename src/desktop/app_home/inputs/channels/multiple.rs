//! z+fW mounted local values. Every inline callback commits, even above max.
use super::*;
fn initial_key(raw: &Value) -> &'static str {
    if raw["type"] == "multi_conversations_select" {
        "initial_conversations"
    } else {
        "initial_channels"
    }
}

pub(super) struct Widget {
    pub(super) conversation: bool,
    pub(super) visit: u64,
    pub(super) dispatch: Option<dispatch::Custody>,
    pub(super) conversation_dispatch: Option<conversation::dispatch::Custody>,
    pub(super) revision: u64,
    pub(super) initial: Option<Value>,
    pub(super) visible: Vec<String>,
    pub(super) retained_choices: BTreeMap<String, directory::Choice>,
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
        .filter(|(_, f)| {
            matches!(
                f.kind,
                InputKind::MultiChannel { .. } | InputKind::MultiConversation { .. }
            )
        })
        .filter_map(|(k, f)| {
            let conversation = matches!(f.kind, InputKind::MultiConversation { .. });
            let form = super::super::form(s)?;
            let custody = if conversation {
                None
            } else {
                Some((form.multi_channel_parent(k).ok()?, dispatch::binding(s, k)?))
            };
            let value = if conversation {
                InputValue::Conversations(form.multi_conversation_seed(k).ok()?)
            } else {
                InputValue::Channels(form.multi_channel_seed(k).ok()?)
            };
            let conversation_custody = if conversation {
                Some((
                    form.multi_conversation_parent(k).ok()?,
                    conversation::dispatch::binding(s, k)?,
                ))
            } else {
                None
            };
            Some((
                k.clone(),
                f.raw.get(initial_key(&f.raw)).cloned(),
                conversation,
                value,
                custody,
                conversation_custody,
            ))
        })
        .collect();
    s.app_home.channels.multiples.retain(|(o, k), _| {
        o.epoch == s.core.account_epoch
            && s.app_home.forms.contains_key(o)
            && (o != &owner || fields.iter().any(|(key, _, _, _, _, _)| key == k))
    });
    for (key, initial, is_conversation, value, custody, conversation_custody) in fields {
        let index = (owner.clone(), key);
        if let Some(w) = s
            .app_home
            .channels
            .multiples
            .get_mut(&index)
            .filter(|w| w.visit == visit && w.conversation == is_conversation)
        {
            if let (Some(current), Some((parent, binding))) = (&mut w.dispatch, custody) {
                current.parent = parent;
                current.binding = binding;
            }
            if let (Some(current), Some((parent, binding))) =
                (&mut w.conversation_dispatch, conversation_custody)
            {
                current.parent = parent;
                current.binding = binding;
            }
            if w.initial != initial {
                // Null/absent initialChannels is falsey; [] is truthy. No emission
                // or internal validity recomputation occurs in this prop effect.
                if let Some(ids) = initial.as_ref().and_then(Value::as_array) {
                    w.visible = ids
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    // Display-only initial props do not revoke labels for the
                    // independently committed values used on a later visit.
                }
                w.initial = initial;
                w.revision = next();
            }
        } else {
            if s.app_home.channels.multiples.len() >= 256
                && !s.app_home.channels.multiples.contains_key(&index)
            {
                continue;
            }
            let (InputValue::Channels(visible) | InputValue::Conversations(visible)) = value else {
                continue;
            };
            let retained_choices = s
                .app_home
                .channels
                .multiples
                .get(&index)
                .filter(|w| w.conversation == is_conversation)
                .map(|w| {
                    w.retained_choices
                        .iter()
                        .filter(|(id, _)| visible.contains(id))
                        .map(|(id, c)| (id.clone(), c.clone()))
                        .collect()
                })
                .unwrap_or_default();
            s.app_home.channels.multiples.insert(
                index,
                Widget {
                    conversation: is_conversation,
                    visit,
                    dispatch: custody.map(|(parent, binding)| dispatch::Custody {
                        instance: next(),
                        parent,
                        binding,
                    }),
                    conversation_dispatch: conversation_custody.map(|(parent, binding)| {
                        conversation::dispatch::Custody {
                            instance: next(),
                            parent,
                            binding,
                        }
                    }),
                    revision: next(),
                    initial,
                    visible,
                    retained_choices,
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
        .channels
        .multiples
        .get(&(owner.clone(), key.clone()))?;
    if w.visit != visit
        || w.initial != raw.get(initial_key(raw)).cloned()
        || w.conversation != (raw["type"] == "multi_conversations_select")
    {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    if w.conversation {
        return Some(
            w.visible
                .iter()
                .flat_map(|id| {
                    conversation::labels(
                        s,
                        &h.owner.ctx.session.team_id,
                        std::iter::once(id),
                        w.retained_choices.get(id),
                    )
                })
                .collect(),
        );
    }
    let workspace = s.core.workspaces.get(&h.owner.ctx.session.team_id)?;
    Some(
        w.visible
            .iter()
            .map(|id| {
                let label = workspace
                    .channels
                    .get(id)
                    .map(|_| conversations::channel_label(workspace, &workspace.channels[id]))
                    .or_else(|| w.retained_choices.get(id).map(|c| c.label.clone()))
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
    let (InputKind::MultiChannel { max_selected_items }
    | InputKind::MultiConversation { max_selected_items }) = field.kind
    else {
        return None;
    };
    let count = displayed.len();
    let parent_error = super::super::presentation_error(s, key);
    Some(Presentation {
        maximum: max_selected_items,
        count,
        invalid: parent_error.is_some() || max_selected_items.is_some_and(|max| count > max),
        parent_error,
        text: max_selected_items.map(|max| {
            let noun = if matches!(field.kind, InputKind::MultiConversation { .. }) {
                "conversations"
            } else {
                "channels"
            };
            format!("You can select up to {max} {noun}.")
        }),
    })
}
pub(super) fn toggle(
    s: &mut ShellState,
    t: &Ticket,
    choice: directory::Choice,
    event: &mut Option<dispatch::Event>,
) -> bool {
    let id = choice.id.as_str();
    if !t.current(s) {
        return false;
    }
    let Some(w) = s
        .app_home
        .channels
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
    edit(s, t, values, Some(choice), event)
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
        .channels
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
    choice: Option<directory::Choice>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) || !super::super::editable(s) {
        return false;
    }
    let Some(field) = super::super::form(s).and_then(|f| f.fields.get(&t.key)) else {
        return false;
    };
    let (InputKind::MultiChannel { max_selected_items }
    | InputKind::MultiConversation { max_selected_items }) = field.kind
    else {
        return false;
    };
    // QjSI uses a truthy max; z+fW uses a defined max. Raw max=0 remains
    // unsupported, but keep the mapped local flag distinct from count guidance.
    let invalid = max_selected_items
        .is_some_and(|max| (t.conversation.is_none() || max != 0) && values.len() > max);
    if t.conversation.is_some() {
        if !conversation::dispatch::multi_input(s, t, values.clone(), event) {
            return false;
        }
    } else {
        if s.app_home
            .channels
            .multiples
            .get(&(t.form.clone(), t.key.clone()))
            .and_then(|w| w.dispatch.as_ref())
            .is_none()
        {
            return false;
        }
        let Some(form) = s.app_home.forms.get_mut(&t.form) else {
            return false;
        };
        let Ok(action) = form.multi_channel_input(&t.key, values.clone()) else {
            return false;
        };
        let Ok(parent) = form.multi_channel_parent(&t.key) else {
            return false;
        };
        super::super::bump(s, &t.form);
        let custody = s
            .app_home
            .channels
            .multiples
            .get_mut(&(t.form.clone(), t.key.clone()))
            .unwrap()
            .dispatch
            .as_mut()
            .unwrap();
        custody.parent = parent;
        *event = action.map(|action| dispatch::Event {
            action,
            binding: custody.binding.clone(),
            instance: custody.instance,
        });
    }
    let w = s
        .app_home
        .channels
        .multiples
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    w.visible = values.clone();
    w.invalid = invalid;
    w.revision = next();
    if let Some(choice) = choice {
        w.retained_choices.insert(choice.id.clone(), choice);
    }
    w.retained_choices.retain(|id, _| w.visible.contains(id));
    // The old callback/request lease is spent by the form update. Keep the
    // current rows and query in the same picker, but retire its pending read.
    let now = t.after_edit(s);
    if let Some(p) = s
        .app_home
        .channels
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
            s.app_home.channels.picker = None;
        }
    }
    let callback = if t.conversation.is_some() {
        json!({"selectedConversations":values})
    } else {
        json!({"selectedChannels":values})
    };
    super::super::static_select::fixture::event(s, &callback, &t.key);
    true
}
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod tests;
