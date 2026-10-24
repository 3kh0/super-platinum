use super::super::{
    Kind, QuipTitle, Status, dependencies_with_emoji_context, label_with_resolvers,
};
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::models::Channel,
    state::directory_channels::{
        Facts, Origin, RendererReply, Store,
        unknown::{Lookup, Manager, Scope},
    },
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

const ID: &str = "CCHANNEL001";
const ENGLISH: Option<ChannelText> = Some(ChannelText::EnglishPlaceholders);
fn owner() -> WorkspaceSession {
    serde_json::from_value(
        json!({"team_id":"TOWNER","user_id":"USELF","name":"Fixture",
        "token":"synthetic","url":"https://fixture.slack.com"}),
    )
    .unwrap()
}
struct Fixture {
    store: Store,
    native: BTreeMap<String, Channel>,
    manager: Manager,
    scope: Scope,
}
impl Fixture {
    fn new() -> Self {
        let mut store = Store::default();
        let native = BTreeMap::new();
        let boot = store.begin_full_boot("A", &owner(), 1).unwrap();
        assert!(store.finish_full_boot(boot, "A", &owner(), 1));
        store.begin("A", &owner(), &native).unwrap();
        let scope = Scope::new("A", &owner(), 1, 7).unwrap();
        Self {
            store,
            native,
            manager: Manager::new(scope.clone()),
            scope,
        }
    }
    fn observed(&self) -> Arc<crate::state::directory_channels::Observation> {
        Arc::new(
            self.store
                .observation("A", &owner(), 1, "q", &self.native)
                .unwrap(),
        )
    }
    fn lookup(&mut self) -> Lookup {
        let observed = self.observed();
        self.manager
            .lookup(
                &self.scope,
                &self.store,
                observed,
                "q",
                &self.native,
                ID,
                true,
            )
            .unwrap()
    }
    fn drain(&mut self) {
        let facts = Facts {
            self_id: "USELF".into(),
            current_team: "TOWNER".into(),
            omit_team: "TOWNER".into(),
            retain_public_membership_on_archive: false,
            members: BTreeMap::new(),
        };
        while self.store.has_pending() {
            if let Some(commit) = self
                .store
                .drain_source_action(&self.observed(), "A", &owner(), 1, "q", &self.native)
                .unwrap()
            {
                (self.store, self.native) = commit.into_parts();
                continue;
            }
            let pending = self
                .store
                .prepare("A", &owner(), 1, "q", &self.native, facts.clone())
                .unwrap();
            let reply = RendererReply {
                generation: "q".into(),
                fields: pending.fields().to_vec(),
                values: pending
                    .fields()
                    .iter()
                    .map(|s| s.to_ascii_lowercase())
                    .collect(),
            };
            (self.store, self.native) = self
                .store
                .finish(pending, &facts, reply, "A", &owner(), 1, "q", &self.native)
                .unwrap()
                .into_parts();
        }
    }
    fn terminal(&mut self) {
        self.lookup();
        let flight = self
            .manager
            .begin(&self.scope, Duration::ZERO)
            .unwrap()
            .unwrap();
        let settled = self.manager.settlement(&self.scope, &flight, &[]).unwrap();
        self.store
            .enqueue_runtime_missing(
                &self.observed(),
                &self.scope,
                "q",
                &self.native,
                settled.missing(),
            )
            .unwrap();
        self.manager.published(&self.scope, settled).unwrap();
        self.drain();
    }
}
fn label(
    file: &Value,
    lookup: &Lookup,
    quip: QuipTitle<'_>,
    context: Option<ChannelText>,
) -> Result<String, Unavailable> {
    label_with_resolvers(
        Kind::Canvas,
        Status::Normal,
        file,
        quip,
        Some("Untitled"),
        None,
        |_| Ok("@Dana".into()),
        |id| observed_channel_label(id, lookup, context),
    )
}

#[test]
fn retained_all_twenty_seven_vectors_use_real_source_lookup_not_marker_seed() {
    let original: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = original["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    let mut count = 0;
    for case in cases {
        let mut f = Fixture::new();
        match case["name"].as_str().unwrap() {
            "absent" => {}
            "unknown" => {
                // Existing runtime_unknown uses a real joined source action.
                f.lookup();
                let flight = f.manager.begin(&f.scope, Duration::ZERO).unwrap().unwrap();
                (f.store, f.native) = f
                    .manager
                    .queue_joined_memberships(
                        &f.scope,
                        &flight,
                        &f.store,
                        &f.observed(),
                        "q",
                        &f.native,
                        &[ID.into()],
                    )
                    .unwrap()
                    .into_parts();
                f.drain();
            }
            "nonexistent" => f.terminal(),
            _ => {
                f.store
                    .enqueue(
                        "A",
                        &owner(),
                        1,
                        Origin::Direct,
                        vec![case["state"]["channels"][ID].clone()],
                    )
                    .unwrap();
                f.drain();
            }
        }
        let lookup = f.lookup();
        assert!(lookup.current(&f.manager, &f.scope, &f.store, "q", &f.native));
        let source = case["file"].clone();
        let before = source.clone();
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            _ => panic!("unknown original override"),
        };
        assert_eq!(
            label(&source, &lookup, quip, ENGLISH).unwrap(),
            case["finalLabel"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let dependencies = dependencies_with_emoji_context(&source, None).unwrap();
        assert_eq!(dependencies.channels.len(), 1);
        assert_eq!(
            dependencies.users.len(),
            case["userIds"].as_array().unwrap().len()
        );
        assert_eq!(source, before);
        if matches!(
            case["name"].as_str().unwrap(),
            "absent" | "unknown" | "nonexistent" | "private-nonmember"
        ) {
            assert!(label(&source, &lookup, quip, None).is_err());
        }
        count += 1;
    }
    assert_eq!(count, 27);
}

#[test]
fn label_context_and_identity_cannot_be_fabricated_by_raw_markers() {
    let mut f = Fixture::new();
    let transient = f.lookup();
    assert_eq!(
        observed_channel_label(ID, &transient, ENGLISH).unwrap(),
        "#unknown-channel"
    );
    assert!(observed_channel_label("COTHER0001", &transient, ENGLISH).is_err());
    assert!(observed_channel_label(ID, &transient, None).is_err());
    for flag in ["isUnknown", "isNonExistent"] {
        let mut raw = json!({"id":ID,"is_channel":true,"name":"never-leak"});
        raw[flag] = true.into();
        assert!(public_channel_label(ID, &raw).is_err());
        assert!(known_ordinary_channel_label(ID, &raw).is_err());
        assert!(
            known_ordinary_channel_label_with_context(
                ID,
                &raw,
                Some(PrivateChannelText::EnglishPlaceholder)
            )
            .is_err()
        );
    }
    f.terminal();
    assert!(!transient.current(&f.manager, &f.scope, &f.store, "q", &f.native));
    let terminal = f.lookup();
    assert_eq!(
        observed_channel_label(ID, &terminal, ENGLISH).unwrap(),
        "#private-channel"
    );
    assert!(observed_channel_label(ID, &terminal, None).is_err());
    // Captured mounted label stays a value; fresh source resolves independently.
    let mounted = observed_channel_label(ID, &terminal, ENGLISH).unwrap();
    f.store
        .enqueue(
            "A",
            &owner(),
            1,
            Origin::Direct,
            vec![json!({"id":ID,"is_channel":true,"name":"restored"})],
        )
        .unwrap();
    f.drain();
    assert!(!terminal.current(&f.manager, &f.scope, &f.store, "q", &f.native));
    assert_eq!(
        observed_channel_label(ID, &f.lookup(), ENGLISH).unwrap(),
        "#restored"
    );
    assert_eq!(mounted, "#private-channel");
}
