//! Actual accepted People preferences, independent of the bounded diagnostic log.
//! Native metadata/routing policy supplies no preference or assignment facts.
use super::*;
use super_platinum_core::{slack::api::people_pages::Sort, slack::events::RtEvent};

const KEYS: [&str; 4] = [
    "result_count_toggle",
    "stick_user_sort",
    "search_people_sort",
    "search_hide_deactivated_users",
];
#[derive(Default)]
pub(super) struct State {
    fields: BTreeMap<&'static str, Arc<Accepted>>,
}
struct Accepted {
    owner: Owner,
    _carrier: Arc<str>,
    value: Option<Value>,
    producer: PreferenceProducer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreferenceProducer {
    Init,
    Socket,
    LocalChoice,
    SubmittedEcho,
}
/// Exact serial adoption edge; it cannot be constructed by a renderer or from
/// matching values. Retired frames remain retired after this edge.
pub(crate) struct SortChange {
    owner: Owner,
    before: Arc<Accepted>,
    after: Arc<Accepted>,
}
/// Secrets remain private in the original shared owner. No Debug/persistence.
#[derive(Clone)]
pub(crate) struct Parameters {
    owner: Owner,
    transformations: Arc<members::Transformations>,
    fields: Vec<(&'static str, Arc<Accepted>)>,
    sort: Sort,
    local_sort: bool,
    hide: bool,
}
fn simple(value: &Value) -> bool {
    value.is_null()
        || value.is_boolean()
        || value.is_number()
        || value.as_str().is_some_and(|s| s.len() <= 128)
}
fn truth(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
pub(super) fn settled(shell: &mut ShellState, owner: &Owner, receipt: &source::Settlement) {
    if !owner.current(shell)
        || receipt.kind() != source::PayloadKind::Init
        || receipt.result().is_err()
    {
        return;
    }
    let team = &owner.context.session.team_id;
    let stores = &shell.core.workspaces[team].cfm_payload.user_prefs;
    let effects: Vec<_> = receipt
        .applied()
        .iter()
        .filter(|e| e.kind == source::Kind::UserPrefs)
        .collect();
    // A successful Init without the responsible prefs carrier supplies no
    // preference observation, even when the native store happens to be empty.
    if effects.is_empty() {
        return;
    }
    let mut observed = Vec::new();
    for key in KEYS {
        let explicit = effects.iter().rev().find_map(|e| e.payload.get(key));
        let value = stores.get(key);
        // Omission cannot certify inherited unknown state. Actual responsible
        // accepted Init absence plus actual store absence can establish falsity.
        if (explicit.is_some() && explicit == value) || (explicit.is_none() && value.is_none()) {
            if value.is_none_or(simple) {
                observed.push((key, value.cloned()));
            }
        }
    }
    let carrier: Arc<str> = receipt.original_bytes().into();
    let state = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .payload
        .people;
    for (key, value) in observed {
        state.fields.insert(
            key,
            Arc::new(Accepted {
                owner: owner.clone(),
                _carrier: carrier.clone(),
                value,
                producer: PreferenceProducer::Init,
            }),
        );
    }
}
pub(crate) fn capture(shell: &ShellState, team: &str) -> Option<Parameters> {
    let context = Context::capture(shell, team).ok()?;
    let slot = shell.app_home.channel_fetches.slots.get(&context.key())?;
    let state = &slot.payload.people;
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
        .map(|key| Some((key, state.fields.get(key)?.clone())))
        .collect::<Option<Vec<_>>>()?;
    let value = |key| {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, a)| a.value.as_ref())
    };
    if truth(value("result_count_toggle")) {
        return None;
    }
    let local_sort = !truth(value("stick_user_sort"));
    let sort = if !local_sort {
        Sort::parse(value("search_people_sort")?.as_str()?)?
    } else {
        Sort::Relevant
    };
    let hide = truth(value("search_hide_deactivated_users"));
    let result = Parameters {
        owner: Owner {
            context: slot.context.clone(),
            token: slot.token.clone(),
        },
        transformations: slot.transformations.clone(),
        fields,
        sort,
        local_sort,
        hide,
    };
    result.current(shell).then_some(result)
}
impl Parameters {
    pub(crate) fn sort_producer(&self) -> PreferenceProducer {
        self.fields
            .iter()
            .find(|(key, _)| *key == "search_people_sort")
            .unwrap()
            .1
            .producer
    }
    /// Called only after the saved-sort controller admits an explicit typed
    /// user choice. This models Source's local setUserPref producer, not RTM.
    pub(crate) fn choose_sort(&self, shell: &mut ShellState, sort: Sort) -> Option<SortChange> {
        if self.local_sort || self.sort == sort {
            return None;
        }
        self.replace_sort(shell, sort, PreferenceProducer::LocalChoice)
    }
    /// A current submitted operation may echo only its original local value.
    /// An ordinary equal socket event cannot satisfy this proof or acknowledge
    /// the pending operation; it replaces the exact local receipt.
    pub(crate) fn submitted_echo(&self, shell: &mut ShellState, sort: Sort) -> Option<SortChange> {
        if self.local_sort
            || self.sort != sort
            || self.sort_producer() != PreferenceProducer::LocalChoice
        {
            return None;
        }
        self.replace_sort(shell, sort, PreferenceProducer::SubmittedEcho)
    }
    fn replace_sort(
        &self,
        shell: &mut ShellState,
        sort: Sort,
        producer: PreferenceProducer,
    ) -> Option<SortChange> {
        if !self.current(shell) {
            return None;
        }
        let before = self
            .fields
            .iter()
            .find(|(key, _)| *key == "search_people_sort")?
            .1
            .clone();
        let value = Value::String(sort.preference_value().into());
        let carrier:Arc<str>=serde_json::json!({"native_producer":match producer {PreferenceProducer::LocalChoice=>"people-sort-choice",PreferenceProducer::SubmittedEcho=>"people-sort-submitted-echo",_=>return None},"name":"search_people_sort","value":sort.preference_value()}).to_string().into();
        let after = Arc::new(Accepted {
            owner: self.owner.clone(),
            _carrier: carrier,
            value: Some(value.clone()),
            producer,
        });
        let store = &mut shell
            .core
            .workspaces
            .get_mut(&self.owner.context.session.team_id)?
            .cfm_payload
            .user_prefs;
        let fields = &mut shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&self.owner.context.key())?
            .payload
            .people
            .fields;
        store.insert("search_people_sort".into(), value);
        fields.insert("search_people_sort", after.clone());
        Some(SortChange {
            owner: self.owner.clone(),
            before,
            after,
        })
    }
    /// Fresh parameters across one exact, genuinely adopted sort-producer edge.
    /// Every unaffected receipt/owner/transformation remains identical. This
    /// returns a NEW read scope; it never changes the validity of the old one.
    pub(crate) fn after_sort_change(
        &self,
        shell: &ShellState,
        change: &SortChange,
    ) -> Option<Self> {
        if !change.owner.current(shell)
            || !Arc::ptr_eq(&change.owner.context, &self.owner.context)
            || !Arc::ptr_eq(&change.owner.token, &self.owner.token)
        {
            return None;
        }
        let fresh = capture(shell, &self.owner.context.session.team_id)?;
        if !Arc::ptr_eq(&fresh.owner.context, &self.owner.context)
            || !Arc::ptr_eq(&fresh.owner.token, &self.owner.token)
            || !Arc::ptr_eq(&fresh.transformations, &self.transformations)
        {
            return None;
        }
        if self.fields.len() != KEYS.len() || fresh.fields.len() != KEYS.len() {
            return None;
        }
        for ((old_key, old), (new_key, new)) in self.fields.iter().zip(&fresh.fields) {
            if old_key != new_key {
                return None;
            }
            if *old_key == "search_people_sort" {
                if !Arc::ptr_eq(old, &change.before) || !Arc::ptr_eq(new, &change.after) {
                    return None;
                }
            } else if !Arc::ptr_eq(old, new) {
                return None;
            }
        }
        Some(fresh)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.owner.current(shell)
            || !shell
                .core
                .client
                .same_configuration(&self.owner.context.client)
        {
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
        let state = &slot.payload.people;
        Arc::ptr_eq(&slot.transformations, &self.transformations)
            && self.fields.iter().all(|(key, accepted)| {
                // The outer owner was checked once for this serial read. These
                // exact owner pointers preserve that check for every receipt.
                Arc::ptr_eq(&accepted.owner.context, &self.owner.context)
                    && Arc::ptr_eq(&accepted.owner.token, &self.owner.token)
                    && state
                        .fields
                        .get(key)
                        .is_some_and(|a| Arc::ptr_eq(a, accepted))
                    && shell.core.workspaces[&self.owner.context.session.team_id]
                        .cfm_payload
                        .user_prefs
                        .get(*key)
                        == accepted.value.as_ref()
            })
    }
    pub(crate) fn sort(&self) -> Sort {
        self.sort
    }
    pub(crate) fn local_sort_allowed(&self) -> bool {
        self.local_sort
    }
    pub(crate) fn effective_sort(&self, local: Sort) -> Sort {
        if self.local_sort { local } else { self.sort }
    }
    pub(crate) fn hide_deactivated(&self) -> bool {
        self.hide
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
/// Called only by the accepted socket dispatcher under its original generation.
/// Equal/ABA values replace the receipt too; old pages cannot regain authority.
pub(crate) fn preference_event(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) {
    let _ = preference_event_with_change(shell, team, generation, event);
}
pub(crate) fn preference_event_with_change(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<SortChange> {
    let RtEvent::PreferencesChanged { name, value } = event else {
        return None;
    };
    let Some(key) = KEYS.into_iter().find(|key| *key == name) else {
        return None;
    };
    let Ok(context) = Context::capture(shell, team) else {
        return None;
    };
    if context.rt_generation != generation {
        return None;
    }
    let Some(slot) = shell.app_home.channel_fetches.slots.get(&context.key()) else {
        return None;
    };
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    if !owner.current(shell) {
        return None;
    }
    if !simple(value) {
        shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&context.key())
            .unwrap()
            .payload
            .people
            .fields
            .remove(key);
        return None;
    }
    let before = slot.payload.people.fields.get(key).cloned();
    let carrier: Arc<str> =
        serde_json::to_string(&serde_json::json!({"type":"pref_change","name":name,"value":value}))
            .unwrap()
            .into();
    let after = Arc::new(Accepted {
        owner: owner.clone(),
        _carrier: carrier,
        value: Some(value.clone()),
        producer: PreferenceProducer::Socket,
    });
    shell
        .core
        .workspaces
        .get_mut(team)
        .unwrap()
        .cfm_payload
        .user_prefs
        .insert(key.into(), value.clone());
    shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&context.key())
        .unwrap()
        .payload
        .people
        .fields
        .insert(key, after.clone());
    if key == "search_people_sort" {
        Some(SortChange {
            owner,
            before: before?,
            after,
        })
    } else {
        None
    }
}
