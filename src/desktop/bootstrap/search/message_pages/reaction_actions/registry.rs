//! Physical reaction bookkeeping compatibility. Eligibility stays Search-owned.
use super::*;
pub(crate) use crate::reaction_writes::{Actor, Key, Outcome, Receipt, Registry, Reservation};
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut banks = std::mem::take(&mut previous.search_reaction_actions.registry);
    banks
        .0
        .extend(std::mem::take(&mut loaded.search_reaction_actions.registry).0);
    loaded.search_reaction_actions.registry = banks;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reaction_actions_retry_after_overflow_holds_actor_across_carry_and_finite_updates() {
        let actor = Actor {
            account: "a".into(),
            team: "T1".into(),
            user: "U1".into(),
        };
        let key = Key {
            actor: actor.clone(),
            channel: "C1".into(),
            ts: "1700000001.000001".into(),
        };
        let mut old = ShellState::fixture(crate::media::MediaRegistry::default());
        let registry = &old.search_reaction_actions.registry;
        let reservation = registry.claim(&key).unwrap();
        reservation.cooldown(Some(u64::MAX));
        reservation.finish(Receipt {
            id: 1,
            name: "wave".into(),
            adding: true,
            outcome: Outcome::Unknown,
        });
        registry.cooldown(&actor, Some(1));
        let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
        carry(&mut old, &mut loaded);
        let registry = &loaded.search_reaction_actions.registry;
        let mut peer = key.clone();
        peer.ts = "1700000002.000001".into();
        assert!(registry.cooling(&actor));
        assert!(
            registry.claim(&peer).is_none(),
            "actor cooldown spans messages and replacement Shell"
        );
        peer.actor.user = "U2".into();
        assert!(!registry.cooling(&peer.actor));
        let reservation = registry.claim(&peer).unwrap();
        reservation.finish(Receipt {
            id: 2,
            name: "wave".into(),
            adding: true,
            outcome: Outcome::NotSent,
        });
        registry.cooldown(&peer.actor, Some(1));
        assert!(
            registry.cooling(&peer.actor),
            "ordinary finite cooldown remains enforced"
        );
        registry.cooldown(&peer.actor, Some(u64::MAX));
        registry.cooldown(&peer.actor, Some(1));
        assert!(
            registry
                .0
                .iter()
                .any(|b| b.lock().unwrap().cooldowns.get(&peer.actor) == Some(&None))
        );
        assert!(registry.claim(&peer).is_none());
    }
}
