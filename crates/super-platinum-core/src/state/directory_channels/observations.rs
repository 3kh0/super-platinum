//! Read acquisition custody while a canonical store is incomplete. An
//! Observation is not Ready and cannot certify a local collection or selection.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryState {
    Unknown,
    CurrentIdentity,
    Overflowed,
}

pub struct Observation {
    stamp: Stamp,
    native: BTreeMap<String, Value>,
    rows: Vec<Value>,
    incoming: Vec<Value>,
    receipt_facts: Option<Facts>,
}
impl Observation {
    /// Already projected source records, in their observed encounter order.
    /// This may be an incomplete collection. Absence is not nonexistence.
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    /// The next real queued batch, before projection. Used to discover exact
    /// member-name dependencies; never supplied as processed local results.
    pub fn incoming(&self) -> &[Value] {
        &self.incoming
    }
    /// Facts frozen at this next batch's actual response, if present. A later
    /// member update cannot rewrite its already accepted source transformation.
    pub fn receipt_facts(&self) -> Option<&Facts> {
        self.receipt_facts.as_ref()
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> bool {
        store.current(&self.stamp, account, owner, epoch, generation)
            && native_snapshot(native).is_ok_and(|v| v == self.native)
    }
}
impl Store {
    /// Same eH source action during incomplete owned acquisition. An absent
    /// left record has old=false and next=false, hence no source mutation;
    /// absent joined records still require their actual channel observation.
    pub fn unchanged_memberships_observed(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        left: &[String],
        joined: &[String],
        allow_private: bool,
    ) -> Result<Commit> {
        if !observed.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        for id in left {
            projection::id(&serde_json::json!({"id":id}))?;
        }
        let ids: BTreeSet<_> = observed
            .rows
            .iter()
            .map(|r| projection::id(r).unwrap())
            .collect();
        let targets: BTreeSet<_> = left.iter().chain(joined).map(String::as_str).collect();
        for record in &self.identity(account, owner)?.records {
            let id = projection::id(&record.source)?;
            if targets.contains(id)
                && !native
                    .get(id)
                    .and_then(|c| serde_json::to_value(c).ok())
                    .is_some_and(|v| source_native_equal(&v, &record.native))
            {
                return Err(Error::MissingObservation);
            }
        }
        let left = left
            .iter()
            .filter(|id| ids.contains(id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        self.apply_memberships(native, &left, joined, allow_private)
    }
    pub fn recovery_state(&self, account: &str, owner: &WorkspaceSession) -> Result<RecoveryState> {
        let expected = Identity::capture(account, owner)?;
        match &self.saved {
            None => Ok(RecoveryState::Unknown),
            Some(saved) if saved.identity != expected => Err(Error::MissingObservation),
            Some(saved) if saved.overflowed => Ok(RecoveryState::Overflowed),
            Some(_) => Ok(RecoveryState::CurrentIdentity),
        }
    }
    pub fn observation(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Observation> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed {
            return Err(Error::MissingObservation);
        }
        Ok(Observation {
            stamp: self.stamp(account, owner, epoch, generation)?,
            native: native_snapshot(native)?,
            rows: saved.records.iter().map(|r| r.source.clone()).collect(),
            receipt_facts: saved.pending.first().and_then(|b| b.receipt_facts.clone()),
            incoming: saved
                .pending
                .first()
                .map(|b| b.rows.clone())
                .unwrap_or_default(),
        })
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
    #[test]
    fn incomplete_observation_is_not_ready_and_new_ingress_retires_it() {
        let owner = owner();
        let mut store = Store::default();
        let native = BTreeMap::from([(
            "CUNKNOWN".into(),
            serde_json::from_value(json!({"id":"CUNKNOWN"})).unwrap(),
        )]);
        assert_eq!(
            store.recovery_state("A", &owner).unwrap(),
            RecoveryState::Unknown
        );
        store.begin("A", &owner, &native).unwrap();
        let lease = store.begin_full_boot("A", &owner, 7).unwrap();
        store
            .enqueue(
                "A",
                &owner,
                7,
                Origin::Boot,
                vec![
                    json!({"id":"CSECOND","name":"second"}),
                    json!({"id":"CFIRST","name":"first"}),
                ],
            )
            .unwrap();
        let observed = store.observation("A", &owner, 7, "query", &native).unwrap();
        assert!(observed.rows().is_empty());
        assert_eq!(observed.incoming()[0]["id"], "CSECOND");
        assert!(observed.current(&store, "A", &owner, 7, "query", &native));
        assert!(store.ready("A", &owner, 7, "query", &native).is_err());
        store
            .enqueue("A", &owner, 7, Origin::Direct, vec![json!({"id":"CLATER"})])
            .unwrap();
        assert!(!observed.current(&store, "A", &owner, 7, "query", &native));
        drop(lease);
    }
    #[test]
    fn observation_rejects_account_credentials_query_and_native_replacement() {
        let owner = owner();
        let native = BTreeMap::new();
        let mut store = Store::default();
        store.begin("A", &owner, &native).unwrap();
        let observed = store.observation("A", &owner, 3, "q", &native).unwrap();
        let mut rotated = owner.clone();
        rotated.token = "different-synthetic".into();
        assert!(store.recovery_state("B", &owner).is_err());
        assert!(!observed.current(&store, "B", &owner, 3, "q", &native));
        assert!(!observed.current(&store, "A", &rotated, 3, "q", &native));
        assert!(!observed.current(&store, "A", &owner, 3, "new-q", &native));
        let changed = BTreeMap::from([(
            "CNEW".into(),
            serde_json::from_value(json!({"id":"CNEW"})).unwrap(),
        )]);
        assert!(!observed.current(&store, "A", &owner, 3, "q", &changed));
    }
    #[test]
    fn unchanged_fresh_source_repairs_its_native_fields_before_certification() {
        let owner = owner();
        let mut store = Store::default();
        let mut native = BTreeMap::new();
        let lease = store.begin_full_boot("A", &owner, 1).unwrap();
        assert!(store.finish_full_boot(lease, "A", &owner, 1));
        store.begin("A", &owner, &native).unwrap();
        let raw = json!({"id":"CONE","name":"one","is_channel":true,"is_member":false,"updated":7});
        let facts = Facts {
            self_id: "USELF".into(),
            current_team: "TOWNER".into(),
            omit_team: "TOWNER".into(),
            retain_public_membership_on_archive: false,
            members: BTreeMap::new(),
        };
        for attempt in 0..2 {
            store
                .enqueue("A", &owner, 1, Origin::Direct, vec![raw.clone()])
                .unwrap();
            let plan = store
                .prepare("A", &owner, 1, "q", &native, facts.clone())
                .unwrap();
            let reply = RendererReply {
                generation: "q".into(),
                fields: plan.fields().to_vec(),
                values: plan
                    .fields()
                    .iter()
                    .map(|v| v.to_ascii_lowercase())
                    .collect(),
            };
            (store, native) = store
                .finish(plan, &facts, reply, "A", &owner, 1, "q", &native)
                .unwrap()
                .into_parts();
            let ready = store.ready("A", &owner, 1, "q", &native).unwrap();
            assert_eq!(ready.rows()[0]["is_member"], false);
            assert_eq!(
                native["CONE"].extra.get("is_member"),
                Some(&Value::Bool(false))
            );
            if attempt == 0 {
                native
                    .get_mut("CONE")
                    .unwrap()
                    .extra
                    .insert("is_member".into(), Value::Bool(true));
                assert!(!ready.current(&store, "A", &owner, 1, "q", &native));
            }
        }
    }
    #[test]
    fn incomplete_membership_custody_does_not_invent_absent_joined_records() {
        let owner = owner();
        let mut store = Store::default();
        let native = BTreeMap::from([(
            "CABSENT".into(),
            serde_json::from_value(json!({"id":"CABSENT"})).unwrap(),
        )]);
        store.begin("A", &owner, &native).unwrap();
        let o = store.observation("A", &owner, 1, "q", &native).unwrap();
        let (same, unchanged) = store
            .unchanged_memberships_observed(
                &o,
                "A",
                &owner,
                1,
                "q",
                &native,
                &["CABSENT".into()],
                &[],
                true,
            )
            .unwrap()
            .into_parts();
        assert!(o.current(&same, "A", &owner, 1, "q", &unchanged));
        assert!(same.ready("A", &owner, 1, "q", &unchanged).is_err());
        assert!(
            store
                .unchanged_memberships_observed(
                    &o,
                    "A",
                    &owner,
                    1,
                    "q",
                    &native,
                    &[],
                    &["CABSENT".into()],
                    true
                )
                .is_err()
        );
        assert!(
            store
                .unchanged_memberships_observed(
                    &o,
                    "A",
                    &owner,
                    1,
                    "q",
                    &native,
                    &["bad".into()],
                    &[],
                    true
                )
                .is_err()
        );
    }
    #[test]
    fn incomplete_membership_action_cannot_restamp_unrelated_drift_on_its_target() {
        let owner = owner();
        let mut store = Store::default();
        let mut native = BTreeMap::new();
        let lease = store.begin_full_boot("A", &owner, 1).unwrap();
        assert!(store.finish_full_boot(lease, "A", &owner, 1));
        store.begin("A", &owner, &native).unwrap();
        store
            .enqueue(
                "A",
                &owner,
                1,
                Origin::Direct,
                vec![json!({"id":"CONE","name":"one","is_channel":true,"is_member":false})],
            )
            .unwrap();
        let facts = Facts {
            self_id: "USELF".into(),
            current_team: "TOWNER".into(),
            omit_team: "TOWNER".into(),
            retain_public_membership_on_archive: false,
            members: BTreeMap::new(),
        };
        let plan = store
            .prepare("A", &owner, 1, "q", &native, facts.clone())
            .unwrap();
        let reply = RendererReply {
            generation: "q".into(),
            fields: plan.fields().to_vec(),
            values: plan
                .fields()
                .iter()
                .map(|v| v.to_ascii_lowercase())
                .collect(),
        };
        (store, native) = store
            .finish(plan, &facts, reply, "A", &owner, 1, "q", &native)
            .unwrap()
            .into_parts();
        let saved = store.cached().unwrap();
        native.get_mut("CONE").unwrap().name = Some("drift".into());
        assert!(store.ready("A", &owner, 1, "q", &native).is_err());
        let observed = store.observation("A", &owner, 1, "q", &native).unwrap();
        assert!(
            store
                .unchanged_memberships_observed(
                    &observed,
                    "A",
                    &owner,
                    1,
                    "q",
                    &native,
                    &[],
                    &["CONE".into()],
                    true
                )
                .is_err()
        );
        assert_eq!(store.cached().unwrap(), saved);
        assert!(store.ready("A", &owner, 1, "q", &native).is_err());
        assert_eq!(
            native["CONE"].extra.get("is_member"),
            Some(&Value::Bool(false))
        );
    }
}
