//! Actual original30-thread/120-actor acquisition, not cached-user promotion.
use super::*;

fn wide_page() -> ThreadsViewPage {
    let threads = (0..30).map(|i| {
        let seconds = 1719800000u64+i*10;
        let root = format!("{seconds}.000000");
        json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"subscribed":true,"user":format!("U{:03}",i*4),"reply_count":3,"text":"Original discussion"},
            "latest_replies":(1..=3).map(|n|json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":format!("{}.{n:06}",seconds+1),"user":format!("U{:03}",i*4+n),"text":"Original reply"})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({"threads":threads,"has_more":false})).unwrap()
}
async fn admit(state: Signal<ShellState>) -> Source {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(wide_page()) })
            .await
            .is_some()
    );
    source
}
fn actor(state: Signal<ShellState>, id: &str) -> Option<Open> {
    let shell = state.peek();
    let source = Source::capture(&shell)?;
    for item in &shell.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.latest_replies)
            .chain(&item.unread_replies)
        {
            if message.user.as_deref() == Some(id) {
                return source.profile(&shell, item, message);
            }
        }
    }
    None
}
fn ids(request: super_platinum_core::slack::client::PreparedRequest) -> Vec<String> {
    let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
        panic!()
    };
    assert_eq!(body["check_interaction"], json!(true));
    assert_eq!(body["include_profile_only_users"], json!(true));
    let values = body["updated_ids"].as_object().unwrap();
    assert!(values.values().all(|v| v == &json!(0)));
    let mut ids = values.keys().cloned().collect::<Vec<_>>();
    ids.sort();
    ids
}
fn reply(ids: &[String]) -> Value {
    json!({"results":ids.iter().map(|id|member(id,&format!("Reader {id}"))).collect::<Vec<_>>()})
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_actor_batches_acquire_every_original_author_and_keep_loaded_data_on_later_failure()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    initial
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert(
            "U119".into(),
            serde_json::from_value(json!({"id":"U119","name":"Cached only"})).unwrap(),
        );
    let before = users(initial);
    let source = admit(initial).await;
    assert!(
        actor(initial, "U119").is_none(),
        "canonical cache alone cannot mint the last actor"
    );
    let requests = Rc::new(RefCell::new(Vec::<Vec<String>>::new()));
    let seen = requests.clone();
    members::load_all_with(source.clone(), initial, move |_, request| {
        let seen = seen.clone();
        async move {
            let ids = ids(request);
            let previous = seen.borrow().len();
            assert_eq!(ids.len(), if previous == 0 { 100 } else { 20 });
            if previous == 1 {
                assert!(
                    actor(initial, "U000").is_some(),
                    "first batch is already published"
                );
                assert!(actor(initial, "U119").is_none());
            }
            seen.borrow_mut().push(ids.clone());
            tokio::task::yield_now().await;
            Ok(reply(&ids))
        }
    })
    .await;
    let lengths = requests.borrow().iter().map(Vec::len).collect::<Vec<_>>();
    assert_eq!(lengths, vec![100, 20]);
    assert_eq!(
        requests.borrow().concat(),
        (0..120).map(|n| format!("U{n:03}")).collect::<Vec<_>>()
    );
    for n in 0..120 {
        assert!(
            actor(initial, &format!("U{n:03}")).is_some(),
            "original actor {n}"
        );
    }
    assert_eq!(users(initial), before);
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let last = actor(state, "U119").unwrap();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Reader U119"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &last.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U119"));
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Read-only profile details"));
    assert_eq!(users(state), before);
    state.write().close_profile();
    let source = admit(state).await;
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    members::load_all_with(source.clone(), state, move |_, request| {
        let ids = ids(request);
        let n = seen.get();
        seen.set(n + 1);
        async move {
            if n == 0 {
                Ok(reply(&ids))
            } else {
                Err(Error::Offline("later batch outage".into()))
            }
        }
    })
    .await;
    assert_eq!(calls.get(), 2);
    assert!(actor(state, "U000").is_some() && actor(state, "U099").is_some());
    assert!(
        actor(state, "U119").is_none(),
        "later failure cannot borrow cached last-user data"
    );
    assert_eq!(users(state), before);
    let duplicate = Rc::new(Cell::new(0));
    let count = duplicate.clone();
    members::load_all_with(source, state, move |_, _| {
        count.set(count.get() + 1);
        async { Err(Error::Offline("duplicate acquisition must not run".into())) }
    })
    .await;
    assert_eq!(
        duplicate.get(),
        0,
        "started chain cannot spawn duplicate requests"
    );
}
