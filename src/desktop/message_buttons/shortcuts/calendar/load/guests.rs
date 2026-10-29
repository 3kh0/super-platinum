//! Whole lookup before metadata, retaining Calendar's cache-first64 finalizer.
use super::*;
use std::future::Future;

pub(in crate::message_buttons::shortcuts::calendar) async fn run_with<L, H, HF>(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    query: String,
    lookup: L,
    hydrate: H,
) where
    L: Future<Output = Result<Option<Vec<User>>, Error>>,
    H: FnOnce(Vec<User>) -> HF,
    HF: Future<Output = ()>,
{
    let result = lookup.await;
    let users = {
        let s = state.peek();
        if !directory::current(&s, id, directory::Query::Guests(generation), &ctx) {
            return;
        }
        if matches!(&result, Ok(None)) {
            drop(s);
            let mut s = state.write();
            if directory::current(&s, id, directory::Query::Guests(generation), &ctx) {
                active_mut(&mut s, id).unwrap().guest_loading = false;
            }
            return;
        }
        let mut users = cached_guest_users(&s, &ctx, &query);
        if let Ok(Some(remote)) = result {
            for user in remote {
                if !users.iter().any(|u| u.id == user.id) {
                    users.push(user);
                }
            }
        }
        users
    };
    if !directory::current(
        &state.peek(),
        id,
        directory::Query::Guests(generation),
        &ctx,
    ) {
        return;
    }
    hydrate(users.clone()).await;
    let mut s = state.write();
    if !directory::current(&s, id, directory::Query::Guests(generation), &ctx) {
        return;
    }
    let found: Vec<Person> = users
        .iter()
        .filter_map(|u| {
            let known = s
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .and_then(|w| w.users.get(&u.id))
                .unwrap_or(u);
            agents::eligible(active(&s, id)?, known, &ctx.session).then(|| Person {
                id: known.id.clone(),
                label: super_platinum_core::state::display_name(Some(known), &known.id),
            })
        })
        .collect();
    let c = active_mut(&mut s, id).unwrap();
    c.guest_loading = false;
    c.guest_choices = found;
    c.guest_choices.truncate(64);
}
