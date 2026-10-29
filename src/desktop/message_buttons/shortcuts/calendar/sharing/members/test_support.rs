use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use crate::bootstrap::test_support::Harness;
use super_platinum_core::slack::client::RequestBody;

pub(super) struct Fixture {
    pub h: Harness,
    pub id: u64,
    pub ctx: Arc<Context>,
    pub renderer: Rc<renderer::Renderer>,
}
pub(super) fn row(id: &str, name: &str, version: u64) -> Value {
    json!({"id":id,"name":name,"updated":version,"profile":{"display_name":name},"is_stranger":false})
}
pub(super) async fn setup() -> Fixture {
    setup_with_normalize(false).await
}
pub(super) async fn setup_with_normalize(normalize: bool) -> Fixture {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install(&mut s);
    super::super::super::fixture::install(&mut s, "app-calendar");
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    d.fixture = false;
    let id = d.id;
    let c = d.calendar.as_mut().unwrap();
    c.fixture = false;
    c.share = true;
    c.guests.clear();
    c.share_generation = 17;
    c.share_query = "Member".into();
    c.share_loading = true;
    c.notice = Some("Unrelated preference warning".into());
    let ctx = context(&s, id).unwrap();
    active_mut(&mut s,id).unwrap().directory_endpoint=Some(Arc::new(DirectoryEndpoint::decode(&ctx.session,&json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{}",ctx.session.team_id)}),Instant::now()).unwrap()));
    let initialization = renderer::assignments(
        &mut s,
        &json!({"ok":true,"assignments":{
        "search_normalize_visible_external_users":{"group":if normalize {"on"} else {"off"}},
        "preserve_omitted_user_change_member_fields":{"group":"on"}}}),
    )
    .unwrap();
    let h = crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s);
    let renderer = Rc::new(renderer::Renderer::default());
    renderer::provide(&h._dom, renderer.clone());
    renderer::run(&h._dom, initialization.drain(h.state))
        .await
        .unwrap();
    assert!(
        intake::Read::capture(
            &h.state.peek(),
            &ctx.session.team_id,
            &member_generation(id, 17)
        )
        .is_ok()
    );
    Fixture {
        h,
        id,
        ctx,
        renderer,
    }
}
impl Fixture {
    pub async fn acquire(&self, rows: Vec<Value>) -> Acquired {
        let id = self.id;
        let ctx = &self.ctx;
        let state = self.h.state;
        let generation = {
            let s = state.peek();
            active(&s, id).unwrap().share_generation
        };
        renderer::run(
            &self.h._dom,
            run_with(state, id, generation, ctx, "Member", |request, scope| {
                let RequestBody::Json(wire) = &request.body else {
                    panic!("JSON")
                };
                assert_eq!(wire["count"], 100);
                assert_eq!(wire["include_profile_only_users"], true);
                let expected = format!(
                    "https://edgeapi.slack.com/cache/{}/users/search",
                    ctx.session.team_id
                );
                assert_eq!(request.url.split('?').next().unwrap(), expected);
                let raw = json!({"ok":true,"results":rows});
                async move { scope.injected(request, |_| async move { Ok(raw) }).await }
            }),
        )
        .await
        .unwrap()
        .unwrap()
    }
    pub async fn external(&self, rows: Vec<Value>) {
        let state = self.h.state;
        let generation = {
            let s = state.peek();
            active(&s, self.id).unwrap().share_generation
        };
        let read = {
            let s = state.peek();
            intake::Read::capture(
                &s,
                &self.ctx.session.team_id,
                "fixture-independent-member-source",
            )
            .unwrap()
        };
        let interactions = Value::Object(
            rows.iter()
                .map(|r| (r["id"].as_str().unwrap().to_owned(), Value::Bool(true)))
                .collect(),
        );
        renderer::run(
            &self.h._dom,
            read.publish_rows(state, &rows, &interactions, |s| {
                owned_current(s, self.id, generation, &self.ctx)
            }),
        )
        .await
        .unwrap();
    }
    pub fn publish(&self, acquired: Acquired) -> Vec<String> {
        let mut state = self.h.state;
        let mut s = state.write();
        let generation = active(&s, self.id).unwrap().share_generation;
        let mut remote = Vec::new();
        assert!(append(
            &mut s,
            self.id,
            generation,
            &self.ctx,
            acquired,
            &mut remote
        ));
        super::super::publish(
            &mut s, self.id, generation, &self.ctx, "Member", remote, false,
        );
        active(&s, self.id)
            .unwrap()
            .share_choices
            .iter()
            .map(|r| r.id.clone())
            .collect()
    }
    pub fn new_query(&self, generation: u64, query: &str) {
        let mut state = self.h.state;
        let mut s = state.write();
        let c = active_mut(&mut s, self.id).unwrap();
        c.share_generation = generation;
        c.share_query = query.into();
        c.share_remote.clear();
        start_query(&mut s, self.id, generation, &self.ctx);
        let selected = active(&s, self.id).unwrap().share_channel.clone();
        let rows = choices(&s, self.id, &self.ctx, query, &selected);
        active_mut(&mut s, self.id).unwrap().share_choices = rows;
    }
    pub fn source(&self) -> Value {
        serde_json::to_value(
            self.h.state.peek().core.workspaces[&self.ctx.session.team_id]
                .directory_members
                .cached(),
        )
        .unwrap()
    }
}
