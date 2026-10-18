//! Accepted raw rows retain their actual ingress-time bootstrap phase while
//! callers resolve dependencies in a private Store/native clone.
use super::*;

/// Transformation provenance only. This does not authorize installation:
/// callers retain and recheck their original Observation and request owner.
/// The rows, origin and observed phase cannot be replaced by a caller.
pub struct AcceptedIngress {
    batch: Batch,
    phase: lifecycle::CapturedPhase,
}

/// Retain alongside the original response Observation until private staging is
/// installed. A private clone alone cannot observe replacement of the live
/// Store's bootstrap attempt.
pub struct IngressWitness {
    phase: lifecycle::CapturedPhase,
}
impl AcceptedIngress {
    /// Exact phase already retained by this accepted batch; no current-phase recapture.
    pub fn boot_in_flight(&self) -> bool {
        self.phase.value()
    }
    pub fn witness(&self) -> IngressWitness {
        IngressWitness {
            phase: self.phase.clone(),
        }
    }
}
impl IngressWitness {
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
    ) -> bool {
        store.identity(account, owner).is_ok()
            && store.lifecycle.accepts(&self.phase, account, owner, epoch)
    }
}

impl Store {
    /// Bind synchronous source facts to this exact accepted batch, before any
    /// renderer or unknown-member await. This preserves source action timing;
    /// it grants no IO/permission/member availability. Caller proves the Member
    /// receipt and experiment observation in this same serial settlement turn.
    pub fn capture_receipt_ingress(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        origin: Origin,
        rows: Vec<Value>,
        facts: Facts,
    ) -> Result<AcceptedIngress> {
        if !valid_receipt_facts(&facts, owner) {
            return Err(Error::MissingObservation);
        }
        let mut accepted = self.capture_ingress(
            observed, account, owner, epoch, generation, native, origin, rows,
        )?;
        accepted.batch.receipt_facts = Some(facts);
        Ok(accepted)
    }
    pub fn capture_ingress(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        origin: Origin,
        rows: Vec<Value>,
    ) -> Result<AcceptedIngress> {
        if origin == Origin::RuntimeMissing
            || !observed.current(self, account, owner, epoch, generation, native)
        {
            return Err(Error::MissingObservation);
        }
        validate_rows(origin, &rows)?;
        let phase = self.lifecycle.capture(account, owner, epoch)?;
        Ok(AcceptedIngress {
            batch: Batch {
                action: None,
                origin,
                rows,
                boot_in_flight: Some(phase.value()),
                receipt_facts: None,
            },
            phase,
        })
    }

    /// For a privately staged clone under the original admission owner. A boot
    /// completion may change current phase; it cannot rewrite accepted facts.
    /// Replacement, cancellation, credentials and account changes refuse.
    pub fn enqueue_ingress(
        &mut self,
        accepted: AcceptedIngress,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
    ) -> Result<()> {
        self.identity(account, owner)?;
        if !self
            .lifecycle
            .accepts(&accepted.phase, account, owner, epoch)
        {
            return Err(Error::MissingObservation);
        }
        self.enqueue_batch(accepted.batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TOWNER".into(),
            user_id: "USELF".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            token: "synthetic".into(),
            url: "https://fixture.slack.com".into(),
        }
    }
    fn accepted(store: &Store) -> AcceptedIngress {
        let native = BTreeMap::new();
        let observed = store.observation("A", &owner(), 1, "q", &native).unwrap();
        store
            .capture_ingress(
                &observed,
                "A",
                &owner(),
                1,
                "q",
                &native,
                Origin::Directory,
                vec![json!({"id":"C00000001","name":"raw"})],
            )
            .unwrap()
    }
    #[test]
    fn accepted_rows_keep_original_phase_through_boot_completion_and_private_dependencies() {
        let mut store = Store::default();
        let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
        store.begin("A", &owner(), &BTreeMap::new()).unwrap();
        let admitted = accepted(&store);
        let mut staged = store.clone();
        assert!(store.finish_full_boot(lease, "A", &owner(), 1));
        staged
            .enqueue(
                "A",
                &owner(),
                1,
                Origin::Direct,
                vec![json!({"id":"C00000002","name":"dependency"})],
            )
            .unwrap();
        staged.enqueue_ingress(admitted, "A", &owner(), 1).unwrap();
        let pending = &staged.saved.as_ref().unwrap().pending;
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].boot_in_flight, Some(false));
        assert_eq!(pending[1].boot_in_flight, Some(true));
        assert_eq!(pending[1].rows[0]["name"], "raw");
        assert!(store.saved.as_ref().unwrap().pending.is_empty());
    }
    #[test]
    fn replacement_or_cancelled_boot_and_changed_credentials_cannot_reuse_receipt() {
        for kind in 0..4 {
            let mut store = Store::default();
            let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
            store.begin("A", &owner(), &BTreeMap::new()).unwrap();
            let admitted = accepted(&store);
            let mut next = owner();
            let mut replacement = None;
            match kind {
                0 => {
                    replacement = Some(store.begin_full_boot("A", &owner(), 1).unwrap());
                }
                1 => drop(lease),
                2 => {
                    assert!(store.finish_full_boot(lease, "A", &owner(), 1));
                    next.token = "rotated".into();
                }
                _ => {
                    assert!(store.finish_full_boot(lease, "A", &owner(), 1));
                }
            }
            assert!(
                store
                    .enqueue_ingress(admitted, if kind == 3 { "B" } else { "A" }, &next, 1)
                    .is_err()
            );
            assert!(store.saved.as_ref().unwrap().pending.is_empty());
            drop(replacement);
        }
    }
    #[test]
    fn stale_response_cannot_capture_a_new_phase() {
        let mut store = Store::default();
        let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
        store.begin("A", &owner(), &BTreeMap::new()).unwrap();
        let native = BTreeMap::new();
        let observed = store.observation("A", &owner(), 1, "q", &native).unwrap();
        store.invalidate("C00000001");
        assert!(
            store
                .capture_ingress(
                    &observed,
                    "A",
                    &owner(),
                    1,
                    "q",
                    &native,
                    Origin::Directory,
                    vec![json!({"id":"C00000001"})]
                )
                .is_err()
        );
        drop(lease);
    }

    #[test]
    fn live_witness_rejects_replacement_even_when_a_private_clone_keeps_old_attempt() {
        let mut live = Store::default();
        let old = live.begin_full_boot("A", &owner(), 1).unwrap();
        live.begin("A", &owner(), &BTreeMap::new()).unwrap();
        let admitted = accepted(&live);
        let witness = admitted.witness();
        let mut staged = live.clone();
        let replacement = live.begin_full_boot("A", &owner(), 1).unwrap();
        staged.enqueue_ingress(admitted, "A", &owner(), 1).unwrap();
        assert!(witness.current(&staged, "A", &owner(), 1));
        assert!(!witness.current(&live, "A", &owner(), 1));
        drop(old);
        drop(replacement);
    }
    #[test]
    fn caller_built_rows_cannot_capture_runtime_marker_ingress() {
        let mut store = Store::default();
        let native = BTreeMap::new();
        let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
        assert!(store.finish_full_boot(lease, "A", &owner(), 1));
        store.begin("A", &owner(), &native).unwrap();
        let observed = store.observation("A", &owner(), 1, "q", &native).unwrap();
        assert!(
            store
                .capture_ingress(
                    &observed,
                    "A",
                    &owner(),
                    1,
                    "q",
                    &native,
                    Origin::RuntimeMissing,
                    vec![json!({"id":"C00000001","isNonExistent":true})]
                )
                .is_err()
        );
        assert!(!store.has_pending());
    }
}

// Ordinary client-v2 constructor identities, not inferred from row affiliation.
pub(super) fn valid_receipt_facts(facts: &Facts, owner: &WorkspaceSession) -> bool {
    facts.self_id == owner.user_id
        && facts.current_team == owner.team_id
        && facts.omit_team == owner.enterprise_id.as_deref().unwrap_or(&owner.team_id)
        && !facts.omit_team.is_empty()
        && facts.members.len() <= 100_000
}

#[cfg(test)]
#[path = "ingress_receipt_tests.rs"]
mod receipt_tests;
