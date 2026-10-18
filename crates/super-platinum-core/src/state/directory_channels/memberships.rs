//! Direct 6OO5.eH -> kV3X.f0 unchanged-membership action. This bypasses the
//! 340b raw-channel processor exactly as the source does; it is not a raw read.
use super::*;
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn private(v: &Value) -> bool {
    (truth(&v["is_private"]) || truth(&v["is_group"])) && !truth(&v["is_mpim"])
}
impl Store {
    /// Caller supplies the same original owned response's resolved-minus-members
    /// and members-minus-returned IDs, after outer query/RT/currentness checks.
    /// Ready is captured before that response. No asynchronous work occurs here.
    /// Unobserved IDs refuse so the caller can acquire them through the actual
    /// channel manager; absence never fabricates a nonmember or placeholder row.
    #[allow(clippy::too_many_arguments)]
    pub fn unchanged_memberships(
        &self,
        ready: &Ready,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        left: &[String],
        joined: &[String],
        allow_private: bool,
    ) -> Result<Commit> {
        if !ready.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        self.apply_memberships(native, left, joined, allow_private)
    }
    pub(super) fn apply_memberships(
        &self,
        native: &BTreeMap<String, Channel>,
        left: &[String],
        joined: &[String],
        allow_private: bool,
    ) -> Result<Commit> {
        // Object construction inserts left keys first, then joined keys override
        // without moving existing slots. All channel IDs are nonnumeric keys.
        let mut wanted = Vec::<(String, bool)>::new();
        let mut positions = BTreeMap::<String, usize>::new();
        for (ids, value) in [(left, false), (joined, true)] {
            for id in ids {
                projection::id(&serde_json::json!({"id":id}))?;
                if let Some(index) = positions.get(id) {
                    wanted[*index].1 = value;
                } else {
                    positions.insert(id.clone(), wanted.len());
                    wanted.push((id.clone(), value));
                }
            }
        }
        let mut store = self.clone();
        let mut native = native.clone();
        let mut changed = false;
        let mut name_updates = Vec::new();
        let saved = store.saved.as_mut().ok_or(Error::MissingObservation)?;
        let indices: BTreeMap<_, _> = saved
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (projection::id(&r.source).unwrap().to_owned(), i))
            .collect();
        for (id, wanted) in wanted {
            let index = *indices.get(&id).ok_or(Error::MissingObservation)?;
            let record = &mut saved.records[index];
            if truth(&record.source["isUnknown"]) || truth(&record.source["isNonExistent"]) {
                return Err(Error::MissingObservation);
            }
            let old = truth(&record.source["is_member"]);
            let next = wanted
                && (!truth(&record.source["is_archived"])
                    || (allow_private && private(&record.source)));
            // Source intentionally does not revoke an already true membership
            // on this unchanged-object path. Returned rows use eV separately.
            if (!old || next) && old != next {
                record.source["is_member"] = Value::Bool(next);
                let channel = native.get_mut(&id).ok_or(Error::MissingObservation)?;
                channel.extra.insert("is_member".into(), Value::Bool(next));
                record.native = serde_json::to_value(channel).map_err(|_| Error::Shape)?;
                name_updates.push(record.source.clone());
                // Keep the last actual wire observation, not a synthetic row.
                changed = true;
            }
        }
        if changed {
            if let Some(index) = &mut store.names {
                // eH dispatches these exact changed objects with a nonempty
                // BULK_UPDATE_CHANNELS reason; names themselves are unchanged.
                Arc::make_mut(index).update(&name_updates, &BTreeMap::new())?;
            }
            store.changed();
        }
        Ok(Commit { store, native })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TONE".into(),
            user_id: "USELF".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        }
    }
    fn settle(store: &mut Store, native: &mut BTreeMap<String, Channel>) {
        let facts = Facts {
            self_id: "USELF".into(),
            current_team: "TONE".into(),
            omit_team: "TONE".into(),
            retain_public_membership_on_archive: false,
            members: BTreeMap::new(),
        };
        let plan = store
            .prepare("A", &owner(), 1, "Q1", native, facts.clone())
            .unwrap();
        assert!(plan.fields().iter().all(|v| v.is_ascii()));
        let reply = RendererReply {
            generation: plan.generation().into(),
            fields: plan.fields().to_vec(),
            values: plan
                .fields()
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
        };
        (*store, *native) = store
            .finish(plan, &facts, reply, "A", &owner(), 1, "Q1", native)
            .unwrap()
            .into_parts();
    }
    fn seeded() -> (Store, BTreeMap<String, Channel>) {
        let mut store = Store::default();
        let mut native = BTreeMap::new();
        let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
        assert!(store.finish_full_boot(lease, "A", &owner(), 1));
        store.begin("A", &owner(), &native).unwrap();
        store
            .enqueue(
                "A",
                &owner(),
                1,
                Origin::Boot,
                vec![
                    serde_json::json!({"id":"CZ","name":"Z","is_channel":true}),
                    serde_json::json!({"id":"CA","name":"A","is_channel":true}),
                ],
            )
            .unwrap();
        settle(&mut store, &mut native);
        (store, native)
    }
    #[test]
    fn unchanged_left_preserves_true_joined_overrides_and_order_stays_stable() {
        let (mut store, mut native) = seeded();
        let ready = store.ready("A", &owner(), 1, "Q1", &native).unwrap();
        let (s, n) = store
            .unchanged_memberships(
                &ready,
                "A",
                &owner(),
                1,
                "Q1",
                &native,
                &["CZ".into()],
                &["CZ".into()],
                true,
            )
            .unwrap()
            .into_parts();
        store = s;
        native = n;
        assert!(!ready.current(&store, "A", &owner(), 1, "Q1", &native));
        assert_eq!(
            native["CZ"].extra.get("is_member"),
            Some(&Value::Bool(true))
        );
        let encoded = serde_json::to_value(&native["CZ"]).unwrap();
        assert_eq!(encoded.get("is_member"), Some(&Value::Bool(true)));
        let reopened: Channel = serde_json::from_value(encoded).unwrap();
        assert_eq!(reopened.extra.get("is_member"), Some(&Value::Bool(true)));
        let ready = store.ready("A", &owner(), 1, "Q1", &native).unwrap();
        let before = store.cached();
        let (s, n) = store
            .unchanged_memberships(
                &ready,
                "A",
                &owner(),
                1,
                "Q1",
                &native,
                &["CZ".into()],
                &[],
                true,
            )
            .unwrap()
            .into_parts();
        store = s;
        native = n;
        assert_eq!(store.cached(), before);
        assert!(ready.current(&store, "A", &owner(), 1, "Q1", &native));
        assert_eq!(
            ready
                .rows()
                .iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["CZ", "CA"]
        );
        // Actual wire history is unchanged, only source/native action fields.
        assert!(
            store.saved.as_ref().unwrap().records[0]
                .wire
                .get("is_member")
                .is_none()
        );
        store
            .enqueue(
                "A",
                &owner(),
                1,
                Origin::Direct,
                vec![serde_json::json!({"id":"CA","is_archived":true})],
            )
            .unwrap();
        settle(&mut store, &mut native);
        assert!(
            store
                .unchanged_memberships(
                    &ready,
                    "A",
                    &owner(),
                    1,
                    "Q1",
                    &native,
                    &[],
                    &["CA".into()],
                    true
                )
                .is_err()
        );
    }
    #[test]
    fn archived_private_exception_excludes_mpim_and_foreign_or_unknown_refuses() {
        for (private, mpim, allow, expected) in [
            (false, false, true, false),
            (true, false, true, true),
            (true, true, true, false),
            (true, false, false, false),
        ] {
            let (mut store, mut native) = seeded();
            store.enqueue("A",&owner(),1,Origin::Direct,vec![serde_json::json!({"id":"CZ","is_archived":true,"is_private":private,"is_mpim":mpim,"members":[]})]).unwrap();
            settle(&mut store, &mut native);
            let ready = store.ready("A", &owner(), 1, "Q1", &native).unwrap();
            assert!(
                store
                    .unchanged_memberships(
                        &ready,
                        "B",
                        &owner(),
                        1,
                        "Q1",
                        &native,
                        &[],
                        &["CZ".into()],
                        allow
                    )
                    .is_err()
            );
            assert!(
                store
                    .unchanged_memberships(
                        &ready,
                        "A",
                        &owner(),
                        1,
                        "Q1",
                        &native,
                        &[],
                        &["CMISSING".into()],
                        allow
                    )
                    .is_err()
            );
            let (s, n) = store
                .unchanged_memberships(
                    &ready,
                    "A",
                    &owner(),
                    1,
                    "Q1",
                    &native,
                    &[],
                    &["CZ".into()],
                    allow,
                )
                .unwrap()
                .into_parts();
            assert_eq!(n["CZ"].extra.get("is_member"), Some(&Value::Bool(expected)));
            assert!(s.ready("A", &owner(), 1, "Q1", &n).is_ok());
        }
    }
}
