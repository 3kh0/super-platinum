//! Dialog-local origins; this is neither canonical readiness nor permanent cache provenance.
use super::*;
const LIMIT: usize = 4096;
const NAME_BYTES: usize = 4096;

#[derive(Clone)]
struct Legacy {
    name: String,
    deleted: bool,
    profile_present: bool,
}
impl Legacy {
    fn capture(user: &User) -> Option<Self> {
        if user.deleted || !valid_id(&user.id) || !cal::sharing_profile_allowed(user) {
            return None;
        }
        let name = super_platinum_core::state::display_name(Some(user), &user.id);
        (name.len() <= NAME_BYTES).then(|| Self {
            name,
            deleted: user.deleted,
            profile_present: user.extra.contains_key("is_profile_only_user"),
        })
    }
    fn matches(&self, user: &User, name: &str) -> bool {
        self.deleted == user.deleted
            && self.name == name
            && self.profile_present == user.extra.contains_key("is_profile_only_user")
            && cal::sharing_profile_allowed(user)
    }
}
#[derive(Clone, Default)]
pub(super) struct Origins {
    entries: BTreeMap<String, Option<Legacy>>,
}
pub(super) struct Registration(Vec<(String, Option<Legacy>)>);
impl Origins {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(super) fn guarded(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }
    pub(super) fn independent(&self, user: &User, name: &str) -> bool {
        self.entries
            .get(&user.id)
            .and_then(Option::as_ref)
            .is_some_and(|w| w.matches(user, name))
    }
    /// Complete validation/budget check precedes every registration or intake.
    pub(super) fn propose(
        &self,
        users: &HashMap<String, User>,
        ids: &[String],
    ) -> Result<Registration, Error> {
        if ids.iter().any(|id| !valid_id(id)) {
            return Err(unavailable());
        }
        let fresh: BTreeSet<_> = ids
            .iter()
            .filter(|id| !self.entries.contains_key(*id))
            .collect();
        if self
            .entries
            .len()
            .checked_add(fresh.len())
            .is_none_or(|n| n > LIMIT)
        {
            return Err(unavailable());
        }
        Ok(Registration(
            fresh
                .into_iter()
                .map(|id| {
                    let witness = users
                        .get(id)
                        .filter(|u| u.id == *id)
                        .and_then(Legacy::capture);
                    (id.clone(), witness)
                })
                .collect(),
        ))
    }
    pub(super) fn commit(&mut self, registration: Registration) {
        for (id, witness) in registration.0 {
            self.entries.entry(id).or_insert(witness);
        }
    }
}
