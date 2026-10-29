use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use crate::bootstrap::test_support::Harness;
use crate::directory_member_fetches::search as member_intake;
use std::{cell::Cell, rc::Rc};
use super_platinum_core::slack::client::RequestBody;
pub(super) const GROUP: &str = "GMPIM0001";
pub(super) const SECOND: &str = "GMPIM0002";
pub(super) const PEER: &str = "UTARGET01";
pub(super) struct Fixture {
    pub h: Harness,
    pub id: u64,
    pub ctx: Arc<Context>,
    pub renderer: Rc<renderer::Renderer>,
}
pub(super) fn row(id: &str, name: &str, kind: bool) -> Value {
    let mut details =
        serde_json::json!({"name":name,"updated":7,"properties":{"has_custom_mpdm_name":true}});
    if kind {
        details["is_mpim"] = Value::Bool(true);
    }
    serde_json::json!({"id":id,"name":"Outer label must not win","members":["UINIT0001",PEER,"UOTHER001"],"details":details})
}
fn seed_group(id: &str, name: &str, members: &[String]) -> Value {
    serde_json::json!({"id":id,"name":name,"is_channel":true,"is_mpim":true,"is_private":true,
        "is_open":true,"is_member":true,"updated":7,"members":members,
        "properties":{"has_custom_mpdm_name":true}})
}
pub(super) async fn setup() -> Fixture {
    setup_with_peers(&[
        (PEER, "New Team Sender", None),
        ("UOTHER001", "Other Peer", None),
    ])
    .await
}
pub(super) async fn setup_with_peers(peers: &[(&str, &str, Option<&str>)]) -> Fixture {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install(&mut s);
    assert!(
        s.core.cache.is_none(),
        "offline fixture must not persist workspace metadata"
    );
    let team = s.core.active_team.clone().unwrap();
    // Compose the final account before the shortcut fixture captures its owner.
    // Retain the channel fixture's disconnected transport and synthetic boot.
    let mut auth = s.core.session.clone().unwrap();
    auth.unified_user_client_enabled.insert(team.clone(), false);
    let transport = s.core.transport.clone().unwrap();
    super::super::super::fixture::install_prepared(&mut s, "app-calendar", move |shell| {
        shell.core.session = Some(auth);
        shell.core.transport = Some(transport);
        renderer::connect(shell).unwrap();
    });
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    d.fixture = false;
    let id = d.id;
    let c = d.calendar.as_mut().unwrap();
    c.fixture = false;
    c.stage = Stage::Editing;
    c.share = true;
    c.guests.clear();
    c.notice = Some("Unrelated preference warning".into());
    let ctx = context(&s, id).unwrap();
    active_mut(&mut s,id).unwrap().directory_endpoint=Some(Arc::new(
        super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint::decode(&ctx.session,
            &serde_json::json!({"ok":true,"ttl_seconds":3600,"url":format!("https://edgeapi.slack.com/cache/{}",team)}),Instant::now()).unwrap()));
    let initialization = renderer::assignments(
        &mut s,
        &serde_json::json!({"ok":true,"assignments":{
        "search_normalize_visible_external_users":{"group":"off"},
        "preserve_omitted_user_change_member_fields":{"group":"on"}}}),
    )
    .unwrap();
    let mut h = crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s);
    let document = Rc::new(renderer::Renderer::default());
    renderer::provide(&h._dom, document.clone());
    let state = h.state;
    renderer::run_pumped(&mut h._dom, initialization.drain(state))
        .await
        .unwrap();
    let read =
        member_intake::Read::capture(&state.peek(), &team, "closed-mpim-fixture-peers").unwrap();
    let members: Vec<String> = std::iter::once(ctx.session.user_id.clone())
        .chain(peers.iter().map(|(id, _, _)| (*id).to_owned()))
        .collect();
    let rows:Vec<Value>=peers.iter().map(|(id,name,real)| {
        let mut profile=serde_json::json!({"display_name":name});
        if let Some(real)=real {profile["real_name"]=Value::String((*real).into());}
        serde_json::json!({"id":id,"name":name,"team_id":team,"updated":1,"is_stranger":false,"profile":profile})
    }).collect();
    let interactions = Value::Object(
        peers
            .iter()
            .map(|(id, _, _)| ((*id).to_owned(), Value::Bool(true)))
            .collect(),
    );
    renderer::run_pumped(
        &mut h._dom,
        read.publish_rows(state, &rows, &interactions, |_| true),
    )
    .await
    .unwrap();
    let init = serde_json::json!({"ok":true,"self":{"id":ctx.session.user_id,"name":"fixture-self","team_id":team,"is_admin":true,"profile":{}},
        "team":{"id":team,"name":"Channel refresh"},"workspaces":[{"id":team}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    let channels = serde_json::json!({"ok":true,"channels":{
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"ims":[],
        "mpims":[seed_group(GROUP,"Old Team",&members),seed_group(SECOND,"Old Team Two",&members)],"is_open":[GROUP,SECOND],
        "has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    let source = renderer::run_pumped(
        &mut h._dom,
        initialization.complete_channels(state, init, channels),
    )
    .await
    .unwrap();
    assert!(source.current_for(&state.peek(), &team));
    assert_eq!(source.row(GROUP).unwrap()["is_open"], true);
    assert_eq!(
        state.peek().core.workspaces[&team].channels[GROUP]
            .name
            .as_deref(),
        Some("Old Team")
    );
    Fixture {
        h,
        id,
        ctx,
        renderer: document,
    }
}
impl Fixture {
    pub fn begin(&self, query: &str) -> u64 {
        let mut state = self.h.state;
        let (ctx, generation) = super::super::begin(&mut state.write(), self.id, query).unwrap();
        assert!(Arc::ptr_eq(&ctx, &self.ctx));
        generation
    }
    pub async fn acquire(&mut self, pages: Vec<Value>) -> (Outcome, usize) {
        self.acquire_results(pages.into_iter().map(Ok).collect())
            .await
    }
    pub async fn acquire_results(&mut self, pages: Vec<Result<Value, Error>>) -> (Outcome, usize) {
        let state = self.h.state;
        let id = self.id;
        let ctx = self.ctx.clone();
        let (generation, query) = {
            let s = state.peek();
            let c = active(&s, id).unwrap();
            (c.share_generation, c.share_query.clone())
        };
        let ingress =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
        let mut pages = pages.into_iter();
        let calls = Cell::new(0);
        let result = renderer::run_pumped(
            &mut self.h._dom,
            read::run_with(
                state,
                id,
                generation,
                &ctx,
                &query,
                |request, scope| {
                    let RequestBody::Json(wire) = &request.body else {
                        panic!("JSON")
                    };
                    assert_eq!(
                        request.url.split('?').next().unwrap(),
                        format!(
                            "https://edgeapi.slack.com/cache/{}/mpims/search",
                            ctx.session.team_id
                        )
                    );
                    assert_eq!(wire["count"], 100);
                    assert_eq!(wire["use_display_name"], true);
                    assert_eq!(wire["fuzz"], 1);
                    assert!(wire.get("include_profile_only_users").is_none());
                    assert!(wire.get("check_membership").is_none());
                    if calls.get() == 0 {
                        assert!(wire.get("marker").is_none());
                    }
                    calls.set(calls.get() + 1);
                    let raw = pages.next().expect("exact finite pages");
                    async move { scope.injected(request, |_| async move { raw }).await }
                },
                move |rows| ingress.channels(&state, rows),
            ),
        )
        .await;
        (result, calls.get())
    }
    pub fn append(&self, outcome: Outcome) -> (Vec<Destination>, bool) {
        let mut state = self.h.state;
        let mut s = state.write();
        let generation = active(&s, self.id).unwrap().share_generation;
        let mut remote: Vec<_> = outcome
            .legacy
            .into_iter()
            .map(Destination::Channel)
            .collect();
        if let Some(acquired) = outcome.acquired {
            assert!(super::append(
                &mut s,
                self.id,
                generation,
                &self.ctx,
                acquired,
                &mut remote
            ));
        }
        (remote, outcome.failed)
    }
    pub fn publish(&self, remote: Vec<Destination>, failed: bool) -> Vec<cal::Choice> {
        let mut state = self.h.state;
        let mut s = state.write();
        let c = active(&s, self.id).unwrap();
        let generation = c.share_generation;
        let query = c.share_query.clone();
        super::super::publish(
            &mut s, self.id, generation, &self.ctx, &query, remote, failed,
        );
        active(&s, self.id).unwrap().share_choices.clone()
    }
    pub fn grant(&self, name: &str, remote: &[Destination]) {
        assert!(
            posting::test_candidates(&self.h.state.peek(), self.id, remote)
                .iter()
                .any(|id| id == GROUP)
        );
        let mut state = self.h.state;
        posting::test_acquire(
            &mut state.write(),
            self.id,
            vec![
                serde_json::json!({"id":"CINIT0001","is_channel":true,"is_member":true,"name":"warm-harbor"}),
                serde_json::json!({"id":GROUP,"is_channel":true,"is_mpim":true,"is_private":true,"is_member":true,"name":name}),
            ],
        );
    }
    pub fn native(&self) -> Value {
        serde_json::to_value(
            &self.h.state.peek().core.workspaces[&self.ctx.session.team_id].channels,
        )
        .unwrap()
    }
    pub fn select(&self, value: &str) {
        super::super::super::actions::edit(
            self.h.state,
            self.id,
            super::super::super::actions::Field::ShareChannel,
            value.into(),
        );
    }
}
