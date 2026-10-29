//! Separate saved Files observations; local Files and People admission stay closed.
use super::*;
use super_platinum_core::slack::{api::file_pages::Sort, events::RtEvent};
const KEYS: [&str; 5] = [
    "result_count_toggle",
    "stick_user_sort",
    "search_file_sort",
    "search_exclude_bots",
    "search_only_my_channels",
];
#[derive(Default)]
pub(super) struct State {
    fields: BTreeMap<&'static str, Arc<Accepted>>,
}
struct Accepted {
    owner: Owner,
    _carrier: Arc<str>,
    value: Option<Value>,
    producer: Producer,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Producer {
    Init,
    Socket,
    LocalChoice,
}
#[derive(Clone)]
pub(crate) struct Parameters {
    owner: Owner,
    transformations: Arc<members::Transformations>,
    fields: Vec<(&'static str, Arc<Accepted>)>,
    exclude: Option<bool>,
    only: Option<bool>,
    sort: Sort,
}
fn simple(v: &Value) -> bool {
    v.is_null() || v.is_boolean() || v.is_number() || v.as_str().is_some_and(|s| s.len() <= 128)
}
fn falsy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => true,
        Some(Value::Number(n)) => n.as_f64() == Some(0.),
        Some(Value::String(s)) => s.is_empty(),
        _ => false,
    }
}
pub(super) fn settled(shell: &mut ShellState, owner: &Owner, receipt: &source::Settlement) {
    if !owner.current(shell)
        || receipt.kind() != source::PayloadKind::Init
        || receipt.result().is_err()
    {
        return;
    }
    let stores = &shell.core.workspaces[&owner.context.session.team_id]
        .cfm_payload
        .user_prefs;
    let effects: Vec<_> = receipt
        .applied()
        .iter()
        .filter(|e| e.kind == source::Kind::UserPrefs)
        .collect();
    if effects.is_empty() {
        return;
    }
    let mut observed = Vec::new();
    for key in KEYS {
        let explicit = effects.iter().rev().find_map(|e| e.payload.get(key));
        let value = stores.get(key);
        if ((explicit.is_some() && explicit == value) || (explicit.is_none() && value.is_none()))
            && value.is_none_or(simple)
        {
            observed.push((key, value.cloned()));
        }
    }
    let carrier: Arc<str> = receipt.original_bytes().into();
    let fields = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .payload
        .file_saved_search
        .fields;
    // An explicitly unsupported new value retires any previous receipt too.
    for key in KEYS {
        if effects.iter().any(|e| e.payload.get(key).is_some()) {
            fields.remove(key);
        }
    }
    for (key, value) in observed {
        fields.insert(
            key,
            Arc::new(Accepted {
                owner: owner.clone(),
                _carrier: carrier.clone(),
                value,
                producer: Producer::Init,
            }),
        );
    }
}
pub(crate) fn capture(shell: &ShellState, team: &str) -> Option<Parameters> {
    let context = Context::capture(shell, team).ok()?;
    let slot = shell.app_home.channel_fetches.slots.get(&context.key())?;
    if slot
        .transformations
        .people_search_assignment
        .as_deref()
        .is_none_or(|s| s == "on")
    {
        return None;
    }
    let fields = KEYS
        .into_iter()
        .map(|key| Some((key, slot.payload.file_saved_search.fields.get(key)?.clone())))
        .collect::<Option<Vec<_>>>()?;
    let value = |key| {
        fields
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, a)| a.value.as_ref())
    };
    if !falsy(value("result_count_toggle")) || value("stick_user_sort") != Some(&Value::Bool(true))
    {
        return None;
    }
    let flag = |key| match value(key) {
        None => Some(None),
        Some(Value::Bool(b)) => Some(Some(*b)),
        _ => None,
    };
    let exclude = flag("search_exclude_bots")?;
    let only = flag("search_only_my_channels")?;
    let raw = value("search_file_sort")?.as_str()?;
    let sort = Sort::ALL.into_iter().find(|s| s.raw() == raw)?;
    let result = Parameters {
        owner: Owner {
            context: slot.context.clone(),
            token: slot.token.clone(),
        },
        transformations: slot.transformations.clone(),
        fields,
        exclude,
        only,
        sort,
    };
    result.current(shell).then_some(result)
}
impl Parameters {
    pub(crate) fn sort(&self) -> Sort {
        self.sort
    }
    pub(crate) fn local_choice(&self) -> bool {
        self.fields
            .iter()
            .find(|(k, _)| *k == "search_file_sort")
            .is_some_and(|(_, a)| a.producer == Producer::LocalChoice)
    }
    /// Only the admitted saved control calls this exact named local producer.
    /// A fresh proof crosses one actual before/after edge; old proofs stay retired.
    pub(crate) fn choose(&self, shell: &mut ShellState, sort: Sort) -> Option<Self> {
        if !self.current(shell) || self.sort == sort {
            return None;
        }
        let value = Value::String(sort.raw().into());
        let after=Arc::new(Accepted { owner:self.owner.clone(),
            _carrier:serde_json::json!({"native_producer":"file-sort-local-choice","name":"search_file_sort","value":sort.raw()}).to_string().into(),
            value:Some(value.clone()),producer:Producer::LocalChoice });
        shell
            .core
            .workspaces
            .get_mut(&self.owner.context.session.team_id)?
            .cfm_payload
            .user_prefs
            .insert("search_file_sort".into(), value);
        shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&self.owner.context.key())?
            .payload
            .file_saved_search
            .fields
            .insert("search_file_sort", after.clone());
        let fresh = capture(shell, &self.owner.context.session.team_id)?;
        if !Arc::ptr_eq(&fresh.owner.context, &self.owner.context)
            || !Arc::ptr_eq(&fresh.owner.token, &self.owner.token)
            || !Arc::ptr_eq(&fresh.transformations, &self.transformations)
        {
            return None;
        }
        for ((old_key, old), (new_key, new)) in self.fields.iter().zip(&fresh.fields) {
            if old_key != new_key
                || if *old_key == "search_file_sort" {
                    !Arc::ptr_eq(new, &after)
                } else {
                    !Arc::ptr_eq(old, new)
                }
            {
                return None;
            }
        }
        Some(fresh)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.owner.current(shell) {
            return false;
        }
        let Some(slot) = shell
            .app_home
            .channel_fetches
            .slots
            .get(&self.owner.context.key())
        else {
            return false;
        };
        Arc::ptr_eq(&slot.transformations, &self.transformations)
            && self.fields.iter().all(|(key, a)| {
                Arc::ptr_eq(&a.owner.context, &self.owner.context)
                    && Arc::ptr_eq(&a.owner.token, &self.owner.token)
                    && slot
                        .payload
                        .file_saved_search
                        .fields
                        .get(key)
                        .is_some_and(|v| Arc::ptr_eq(v, a))
                    && shell.core.workspaces[&self.owner.context.session.team_id]
                        .cfm_payload
                        .user_prefs
                        .get(*key)
                        == a.value.as_ref()
            })
    }
    pub(crate) fn exclude_bots(&self) -> Option<bool> {
        self.exclude
    }
    pub(crate) fn only_my_channels(&self) -> Option<bool> {
        self.only
    }
    pub(crate) fn client(&self) -> &SlackClient {
        &self.owner.context.client
    }
    pub(crate) fn session(&self) -> &WorkspaceSession {
        &self.owner.context.session
    }
    pub(crate) fn transport(&self) -> &Arc<Transport> {
        &self.owner.context.transport
    }
}
/// Original accepted socket generation, including equal/ABA updates. No diagnostic
/// log, renderer DTO or value equality can create a current receipt.
pub(crate) fn preference_event(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
    original: Option<&super_platinum_core::slack::realtime::PreferenceCarrier>,
) {
    let RtEvent::PreferencesChanged { name, value } = event else {
        return;
    };
    let Some(key) = KEYS.into_iter().find(|k| *k == name) else {
        return;
    };
    let Ok(context) = Context::capture(shell, team) else {
        return;
    };
    if context.rt_generation != generation {
        return;
    }
    let Some(slot) = shell.app_home.channel_fetches.slots.get(&context.key()) else {
        return;
    };
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    if !owner.current(shell) {
        return;
    }
    // Even an unsupported/unretained value updates the actual post-store map;
    // a later omitted Init key must not certify a fabricated absence.
    shell
        .core
        .workspaces
        .get_mut(team)
        .unwrap()
        .cfm_payload
        .user_prefs
        .insert(key.into(), value.clone());
    let fields = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&context.key())
        .unwrap()
        .payload
        .file_saved_search
        .fields;
    fields.remove(key);
    let Some(original) = original.filter(
        |c| matches!(c.event(), RtEvent::PreferencesChanged{name:n,value:v} if n==name&&v==value),
    ) else {
        return;
    };
    if !simple(value) {
        return;
    }
    let carrier = original.original();
    fields.insert(
        key,
        Arc::new(Accepted {
            owner,
            _carrier: carrier,
            value: Some(value.clone()),
            producer: Producer::Socket,
        }),
    );
    shell
        .core
        .workspaces
        .get_mut(team)
        .unwrap()
        .cfm_payload
        .user_prefs
        .insert(key.into(), value.clone());
}
