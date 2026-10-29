use super::*;
use super_platinum_core::slack::{PreparedRequest, models::User};
pub(super) fn person(s: &ShellState, ctx: &Context, id: &str) -> Option<Person> {
    let w = s.core.workspaces.get(&ctx.session.team_id)?;
    let u = w.users.get(id)?;
    let c = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()?
        .calendar
        .as_ref()?;
    agents::eligible(c, u, &ctx.session).then(|| Person {
        id: id.into(),
        label: super_platinum_core::state::display_name(Some(u), id),
    })
}
pub(super) fn eligible(u: &User, self_id: &str) -> bool {
    base_eligible(u, self_id)
        && !super_platinum_core::block_actions::app_eligibility::agents::slackbot::is_slackbot(u)
        && !u.is_bot
        && u.extra.get("is_app_user") != Some(&serde_json::Value::Bool(true))
        && u.extra.get("is_agentforce_bot") != Some(&serde_json::Value::Bool(true))
}
pub(super) fn base_eligible(u: &User, self_id: &str) -> bool {
    u.id != self_id
        && [
            "is_app_user",
            "is_agentforce_bot",
            "is_slackbot",
            "is_workflow_bot",
            "is_profile_only_user",
            "is_slacksecuritybot",
        ]
        .iter()
        .all(|key| matches!(u.extra.get(*key), None | Some(serde_json::Value::Bool(_))))
        && !u.deleted
        && super_platinum_core::block_actions::form::valid_message_user(&u.id)
        && !matches!(u.id.as_str(), "USEARCH" | "UCHANNELEXPERT" | "USLACK")
        && ![
            "is_workflow_bot",
            "is_profile_only_user",
            "is_slacksecuritybot",
        ]
        .iter()
        .any(|key| u.extra.get(*key) == Some(&serde_json::Value::Bool(true)))
        && !(u.is_restricted
            && u.extra
                .get("restricted_user_type")
                .and_then(serde_json::Value::as_str)
                == Some("external_guest"))
}
fn initial_users(s: &ShellState, id: u64) -> Vec<User> {
    let Some(d) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .filter(|d| d.id == id)
    else {
        return vec![];
    };
    let Some(w) = s.core.workspaces.get(&d.owner.ctx.session.team_id) else {
        return vec![];
    };
    let Some(ch) = w.channels.get(&d.owner.channel) else {
        return vec![];
    };
    let mut ids = Vec::new();
    if let Some((root, _)) = &d.owner.thread {
        if let Some(message) = w
            .messages
            .get(&d.owner.channel)
            .and_then(|bag| bag.messages.iter().find(|m| m.ts.as_ref() == Some(root)))
        {
            ids.extend(message.reply_users.iter().cloned());
        }
    } else if ch.is_im {
        if let Some(user) = &ch.user {
            ids.push(user.clone());
        }
    } else if ch.is_mpim {
        if let Some(users) = ch
            .extra
            .get("members")
            .and_then(serde_json::Value::as_array)
        {
            ids.extend(users.iter().filter_map(|v| v.as_str().map(str::to_owned)));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .filter_map(|id| w.users.get(&id).cloned())
        .collect()
}
pub(super) fn initial_guests(s: &ShellState, id: u64) -> Vec<Person> {
    let Some(ctx) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .filter(|d| d.id == id)
        .map(|d| &d.owner.ctx)
    else {
        return vec![];
    };
    initial_users(s, id)
        .into_iter()
        .filter(|u| eligible(u, &ctx.session.user_id))
        .map(|u| Person {
            id: u.id.clone(),
            label: super_platinum_core::state::display_name(Some(&u), &u.id),
        })
        .collect()
}
async fn initial_agent_guests(mut state: Signal<ShellState>, id: u64, ctx: &Context) {
    let users = initial_users(&state.peek(), id);
    agents::hydrate(state, id, 0, ctx, users.clone()).await;
    let mut s = state.write();
    let Some(c) = active_mut(&mut s, id).filter(|c| c.stage == Stage::Loading) else {
        return;
    };
    c.guests = users
        .iter()
        .filter(|u| agents::eligible(c, u, &ctx.session))
        .map(|u| Person {
            id: u.id.clone(),
            label: super_platinum_core::state::display_name(Some(u), &u.id),
        })
        .collect();
}
pub(super) fn share_choices(
    s: &ShellState,
    ctx: &Context,
    query: &str,
    selected: &str,
) -> Vec<cal::Choice> {
    share_choices_admitted(s, ctx, query, selected, |_, _| true)
}
pub(super) fn share_choices_admitted(
    s: &ShellState,
    ctx: &Context,
    query: &str,
    selected: &str,
    admitted: impl FnMut(&User, &str) -> bool,
) -> Vec<cal::Choice> {
    share_choices_with_admission(s, ctx, query, selected, admitted, |_, _| true)
}
pub(super) fn share_channel_name(
    s: &ShellState,
    ctx: &Context,
    ch: &super_platinum_core::slack::models::Channel,
) -> String {
    let w = s.core.workspaces.get(&ctx.session.team_id);
    ch.name
        .clone()
        .or_else(|| {
            ch.user.as_ref().map(|id| {
                super_platinum_core::state::display_name(w.and_then(|w| w.users.get(id)), id)
            })
        })
        .unwrap_or_else(|| ch.id.clone())
}
pub(super) fn share_choices_with_admission(
    s: &ShellState,
    ctx: &Context,
    query: &str,
    selected: &str,
    mut admitted: impl FnMut(&User, &str) -> bool,
    mut channel_admitted: impl FnMut(&super_platinum_core::slack::models::Channel, &str) -> bool,
) -> Vec<cal::Choice> {
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return vec![];
    };
    let q = query.trim().to_lowercase();
    let mut out: Vec<cal::Choice> = Vec::new();
    let mut offer = |v: cal::Choice| {
        if v.id != selected && !q.is_empty() && !v.name.to_lowercase().contains(&q) {
            return;
        }
        let at = out.partition_point(|old| {
            ((old.id != selected), &old.name, &old.id) <= ((v.id != selected), &v.name, &v.id)
        });
        if at < 100 {
            out.insert(at, v);
            out.truncate(100);
        }
    };
    for ch in w.channels.values() {
        if sharing::channel(s, ctx, ch) {
            let name = share_channel_name(s, ctx, ch);
            if channel_admitted(ch, &name) {
                offer(cal::Choice {
                    id: ch.id.clone(),
                    name,
                });
            }
        }
    }
    for u in w.users.values() {
        if !u.deleted
            && domain::identifier(&u.id)
            && (u.id == selected || cal::sharing_profile_allowed(u))
        {
            let name = super_platinum_core::state::display_name(Some(u), &u.id);
            if admitted(u, &name) {
                offer(cal::Choice {
                    id: u.id.clone(),
                    name,
                });
            }
        }
    }
    out
}
pub(super) async fn read(
    mut state: Signal<ShellState>,
    id: u64,
    ctx: &Context,
    request: PreparedRequest,
) -> Result<serde_json::Value, Error> {
    {
        let s = state.peek();
        if !current(&s, id) || cooling(&s, ctx) {
            return Err(Error::Api("calendar_read_retired".into()));
        }
    }
    let result = ctx.transport.execute(request).await;
    let mut s = state.write();
    retain_limit(&mut s, ctx, result.as_ref().err());
    if !current(&s, id) {
        return Err(Error::Api("calendar_read_retired".into()));
    }
    result
}
pub(super) fn start(mut state: Signal<ShellState>, id: u64, ctx: Arc<Context>) {
    dioxus::core::spawn_forever(async move {
        let provider = {
            let s = state.peek();
            let Some(c) = active(&s, id) else {
                return;
            };
            c.provider
        };
        let result:Result<bool,Error>=async{
            let assignments=read(state,id,&ctx,api::list_experiments(&ctx.client,&ctx.session)).await?;
            let flag=cal::modern(&assignments)?;
            let account=if flag {cal::Account::Modern}else{
                let raw=read(state,id,&ctx,cal::user_info(&ctx.client,&ctx.session,provider)).await?;
                match cal::connection(&raw)?{
                    cal::Connection::Connected(a)=>a,
                    cal::Connection::Connect{url}=>{if let Some(c)=active_mut(&mut state.write(),id){c.auth_url=Some(url);c.stage=Stage::Connect;}return Ok(false);},
                    cal::Connection::Unavailable=>return Err(cal::invalid()),
                }
            };
            let calendars=cal::calendars(&read(state,id,&ctx,cal::connected_calendars(&ctx.client,&ctx.session,provider,&account)).await?)?;
            // Conference discovery failure does not discard a usable Calendar draft.
            let conferences=read(state,id,&ctx,cal::conference_apps(&ctx.client,&ctx.session)).await.and_then(|r|cal::conferences(&r));
            if let Some(c)=active_mut(&mut state.write(),id){
                c.account=Some(account);c.calendars=calendars;
                match conferences{Ok(v)=>c.conferences=v,Err(_)=>c.notice=Some("Conference apps could not be loaded. You can create an event without a conference.".into())}
            }else{return Ok(false);}
            let legacy={let s=state.peek();active(&s,id).is_some_and(|c|matches!(c.account,Some(cal::Account::Legacy{..})))};
            if legacy{
                let preference=read(state,id,&ctx,cal::share_preference(&ctx.client,&ctx.session)).await.and_then(|r|cal::share_enabled(&r));
                if let Some(c)=active_mut(&mut state.write(),id){match preference{Ok(value)=>c.share=value,Err(_)=>{c.share=false;c.notice=Some("Sharing preference could not be loaded. Sharing is off for this draft until you choose it.".into());}}}
            }
            initial_agent_guests(state,id,&ctx).await;
            default_guests(state,id,&ctx).await;
            if let Some(c)=active_mut(&mut state.write(),id){c.stage=Stage::Editing;}Ok(true)
        }.await;
        match result {
            Ok(true) => {
                slots(state, id);
                guests(state, id, String::new());
                let legacy = {
                    let s = state.peek();
                    active(&s, id)
                        .is_some_and(|c| matches!(c.account, Some(cal::Account::Legacy { .. })))
                };
                if legacy {
                    sharing::search(state, id, String::new());
                }
            }
            Ok(false) => {}
            Err(_) => {
                if let Some(c) = active_mut(&mut state.write(), id) {
                    c.stage = Stage::Failed;
                    c.notice = Some(
                        "Calendar information could not be loaded. No event was created.".into(),
                    );
                }
            }
        }
    });
}
async fn default_guests(mut state: Signal<ShellState>, id: u64, ctx: &Context) {
    let channel = {
        let s = state.peek();
        let Some(c) = active(&s, id) else {
            return;
        };
        if !c.guests.is_empty() {
            return;
        }
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .owner
            .channel
            .clone()
    };
    let request = ctx.client.rest_form(
        &ctx.session,
        "conversations.info",
        vec![
            ("channel", channel.clone()),
            ("include_num_members", "true".into()),
        ],
    );
    let Ok(raw) = read(state, id, ctx, request).await else {
        return;
    };
    let Some(count) = raw
        .get("channel")
        .and_then(|v| v.get("num_members"))
        .and_then(serde_json::Value::as_u64)
    else {
        return;
    };
    if count >= 10 {
        return;
    }
    if !current(&state.peek(), id) {
        return;
    }
    let result =
        api::fetch_conversation_members(&ctx.transport, &ctx.client, &ctx.session, &channel, None)
            .await;
    if let Ok(page) = &result {
        if page.members.len() >= 10
            || page
                .response_metadata
                .as_ref()
                .and_then(|m| m.next_cursor.as_ref())
                .is_some_and(|c| !c.is_empty())
        {
            return;
        }
        let users = {
            let s = state.peek();
            let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
                return;
            };
            page.members
                .iter()
                .filter_map(|id| {
                    w.users
                        .get(id)
                        .or_else(|| page.users.iter().find(|u| u.id == *id))
                        .cloned()
                })
                .collect()
        };
        agents::hydrate(state, id, 0, ctx, users).await;
    }
    let mut s = state.write();
    retain_limit(&mut s, ctx, result.as_ref().err());
    if !current(&s, id) {
        return;
    }
    if let Ok(page) = result {
        if page.members.len() >= 10
            || page
                .response_metadata
                .as_ref()
                .and_then(|m| m.next_cursor.as_ref())
                .is_some_and(|c| !c.is_empty())
        {
            return;
        }
        let guests = page
            .members
            .iter()
            .filter_map(|member| {
                person(&s, ctx, member).or_else(|| {
                    page.users
                        .iter()
                        .find(|u| {
                            u.id == *member
                                && active(&s, id)
                                    .is_some_and(|c| agents::eligible(c, u, &ctx.session))
                        })
                        .map(|u| Person {
                            id: member.clone(),
                            label: super_platinum_core::state::display_name(Some(u), member),
                        })
                })
            })
            .collect();
        if let Some(c) = active_mut(&mut s, id) {
            c.guests = guests;
        }
    }
}
pub(super) fn slots(mut state: Signal<ShellState>, id: u64) {
    let job = {
        let mut s = state.write();
        if !editable(&s, id) {
            return;
        }
        let ctx = context(&s, id).unwrap();
        let c = active_mut(&mut s, id).unwrap();
        c.slot_generation = c.slot_generation.wrapping_add(1);
        c.slot_notice = None;
        if c.custom {
            c.slot_loading = false;
            c.slots = cal::time::custom_slots(&c.date, c.duration, cal::time::now())
                .into_iter()
                .map(|(start, end)| cal::Slot {
                    start,
                    end,
                    all_free: true,
                    busy: vec![],
                })
                .collect();
            return;
        }
        let Some(account) = c.account.as_ref() else {
            return;
        };
        let Ok(request) = cal::freebusy(
            &ctx.client,
            &ctx.session,
            c.provider,
            account,
            &c.date,
            c.duration,
            &c.guests.iter().map(|g| g.id.clone()).collect::<Vec<_>>(),
        ) else {
            c.slot_loading = false;
            c.slot_notice = Some("Choose a valid date and duration.".into());
            return;
        };
        c.slot_loading = true;
        c.slots.clear();
        c.start = None;
        c.end = None;
        c.start_text.clear();
        c.end_text.clear();
        (ctx, c.slot_generation, request)
    };
    dioxus::core::spawn_forever(async move {
        let (ctx, generation, request) = job;
        if !active(&state.peek(), id)
            .is_some_and(|c| c.slot_generation == generation && c.stage == Stage::Editing)
        {
            return;
        }
        let result = read(state, id, &ctx, request)
            .await
            .and_then(|r| cal::slots(&r));
        let mut s = state.write();
        let Some(c) = active_mut(&mut s, id)
            .filter(|c| c.slot_generation == generation && c.stage == Stage::Editing)
        else {
            return;
        };
        c.slot_loading = false;
        match result{Ok(slots)=>{c.slots=slots;if !c.slots.iter().any(|slot|Some(slot.start)==c.start&&slot.all_free){if let Some(slot)=c.slots.first(){c.start=Some(slot.start);c.end=Some(slot.end);c.start_text=cal::time::label(slot.start);c.end_text=cal::time::label(slot.end);}}},Err(_)=>c.slot_notice=Some("Availability could not be loaded. Choose a custom time or refresh after the rate limit.".into())}
    });
}
pub(super) mod guests;
pub(super) fn guests(mut state: Signal<ShellState>, id: u64, query: String) {
    let job = {
        let mut s = state.write();
        if !editable(&s, id) || query.len() > 4096 {
            return;
        }
        let ctx = context(&s, id).unwrap();
        let cached = cached_guests(&s, &ctx, &query);
        let c = active_mut(&mut s, id).unwrap();
        c.guest_query = query.clone();
        c.guest_choices = cached;
        c.guest_generation = c.guest_generation.wrapping_add(1);
        c.guest_loading = true;
        (ctx, c.guest_generation)
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_millis(180)).await;
        let (ctx, generation) = job;
        if !active(&state.peek(), id)
            .is_some_and(|c| c.guest_generation == generation && c.stage == Stage::Editing)
        {
            return;
        }
        let lookup = directory::guests::read(state, id, generation, ctx.clone(), &query);
        let original = ctx.clone();
        guests::run_with(
            state,
            id,
            generation,
            ctx.clone(),
            query.clone(),
            lookup,
            move |users| async move {
                agents::hydrate(state, id, generation, &original, users).await;
            },
        )
        .await;
    });
}
fn cached_guests(s: &ShellState, ctx: &Context, query: &str) -> Vec<Person> {
    let Some(c) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .and_then(|d| d.calendar.as_ref())
    else {
        return vec![];
    };
    cached_guest_users(s, ctx, query)
        .into_iter()
        .filter(|u| agents::eligible(c, u, &ctx.session))
        .map(|u| Person {
            id: u.id.clone(),
            label: super_platinum_core::state::display_name(Some(&u), &u.id),
        })
        .collect()
}
fn cached_guest_users(s: &ShellState, ctx: &Context, query: &str) -> Vec<User> {
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return vec![];
    };
    let q = query.trim().to_lowercase();
    let mut rows: Vec<(String, User)> = Vec::with_capacity(65);
    for u in w
        .users
        .values()
        .filter(|u| base_eligible(u, &ctx.session.user_id))
    {
        let label = super_platinum_core::state::display_name(Some(u), &u.id);
        if !q.is_empty() && !label.to_lowercase().contains(&q) {
            continue;
        }
        let at = rows.partition_point(|(old_label, old)| (old_label, &old.id) <= (&label, &u.id));
        if at < 64 {
            rows.insert(at, (label, u.clone()));
            rows.truncate(64);
        }
    }
    rows.into_iter().map(|(_, u)| u).collect()
}
