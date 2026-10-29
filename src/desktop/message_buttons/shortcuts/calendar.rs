//! Client-owned Calendar surface. Ordinary app-modal correlation is not involved.
use super::*;
use super_platinum_core::{block_actions::shortcuts::calendar as cal, slack::api};
mod actions;
mod agents;
mod directory;
mod fixture;
mod native;
pub(super) fn install_fixture(s: &mut ShellState, variant: &str) {
    fixture::install(s, variant);
}
mod load;
mod metadata;
mod navigation;
mod posting;
mod sharing;
pub(super) fn home_navigation_current(s: &ShellState, id: u64) -> bool {
    navigation::current(s, id)
}
pub(super) fn close_home_navigation(s: &mut ShellState, id: u64) {
    navigation::close_current(s, id)
}
#[cfg(test)]
pub(super) fn navigation_test_setup(s: &mut ShellState) -> u64 {
    fixture::install(s, "app-calendar");
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    d.fixture = false;
    let c = d.calendar.as_mut().unwrap();
    c.fixture = false;
    c.stage = Stage::Created;
    d.id
}
mod view;
mod writes;
pub(super) fn dialog(state: Signal<ShellState>, id: u64) -> Element {
    view::dialog(state, id)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Loading,
    Connect,
    Editing,
    Creating,
    Created,
    Uncertain,
    Failed,
}
#[derive(Clone)]
struct Person {
    id: String,
    label: String,
}
#[derive(Clone)]
pub(super) struct Calendar {
    provider: cal::Provider,
    metadata: metadata::Metadata,
    posting: posting::Facts,
    agents: agents::Facts,
    directory_endpoint:
        Option<Arc<super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint>>,
    stage: Stage,
    account: Option<cal::Account>,
    auth_url: Option<String>,
    calendars: Vec<cal::Choice>,
    conferences: Vec<cal::Choice>,
    rooms: BTreeMap<String, String>,
    room_pending: Option<u64>,
    room_unknown: bool,
    selected_room: String,
    title: String,
    description: String,
    date: String,
    time_revision: u64,
    start_text: String,
    end_text: String,
    start: Option<i64>,
    end: Option<i64>,
    duration: u16,
    custom: bool,
    slots: Vec<cal::Slot>,
    slot_generation: u64,
    slot_loading: bool,
    slot_notice: Option<String>,
    calendar: String,
    guests: Vec<Person>,
    guest_query: String,
    guest_choices: Vec<Person>,
    guest_generation: u64,
    guest_loading: bool,
    share: bool,
    share_channel: String,
    share_choices: Vec<cal::Choice>,
    share_query: String,
    share_generation: u64,
    share_loading: bool,
    share_selected: Option<sharing::Destination>,
    share_remote: Vec<sharing::Destination>,
    members: sharing::MemberState,
    mpims: sharing::MpimState,
    preference_pending: Option<u64>,
    notice: Option<String>,
    confirm_close: bool,
    provider_link: Option<String>,
    app_link: Option<String>,
    attempt: Option<u64>,
    fixture: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Write {
    Event(cal::Event),
    Room { app: String, channel: String },
    Preference(bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Settlement {
    Sending,
    Created(Option<String>),
    Rejected,
    Unsent,
    Unknown,
}
#[derive(Clone)]
pub(super) struct Attempt {
    id: u64,
    epoch: u64,
    team: String,
    provider: cal::Provider,
    account: Option<cal::Account>,
    write: Write,
    result: Settlement,
}
impl Calendar {
    fn new(
        provider: cal::Provider,
        channel: String,
        guests: Vec<Person>,
        choices: Vec<cal::Choice>,
    ) -> Self {
        Self {
            provider,
            metadata: metadata::Metadata::default(),
            posting: posting::Facts::default(),
            agents: agents::Facts::default(),
            directory_endpoint: None,
            stage: Stage::Loading,
            account: None,
            auth_url: None,
            calendars: vec![],
            conferences: vec![],
            rooms: BTreeMap::new(),
            room_pending: None,
            room_unknown: false,
            selected_room: String::new(),
            title: String::new(),
            description: String::new(),
            date: cal::time::today(),
            time_revision: 0,
            start_text: String::new(),
            end_text: String::new(),
            start: None,
            end: None,
            duration: 30,
            custom: false,
            slots: vec![],
            slot_generation: 0,
            slot_loading: false,
            slot_notice: None,
            calendar: String::new(),
            guests,
            guest_query: String::new(),
            guest_choices: vec![],
            guest_generation: 0,
            guest_loading: false,
            share: false,
            share_channel: channel,
            share_choices: choices,
            share_query: String::new(),
            share_generation: 0,
            share_loading: false,
            share_selected: None,
            share_remote: vec![],
            members: sharing::MemberState::default(),
            mpims: sharing::MpimState::default(),
            preference_pending: None,
            notice: None,
            confirm_close: false,
            provider_link: None,
            app_link: None,
            attempt: None,
            fixture: false,
        }
    }
    fn dirty(&self) -> bool {
        !self.title.is_empty() || !self.description.is_empty() || !self.selected_room.is_empty()
    }
}
fn current(s: &ShellState, id: u64) -> bool {
    is_current(s, id)
        && s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .is_some_and(|d| d.calendar.is_some())
}
fn active(s: &ShellState, id: u64) -> Option<&Calendar> {
    current(s, id)
        .then(|| {
            s.message_buttons
                .shortcuts
                .dialog
                .as_ref()?
                .calendar
                .as_ref()
        })
        .flatten()
}
fn active_mut(s: &mut ShellState, id: u64) -> Option<&mut Calendar> {
    if !current(s, id) {
        return None;
    }
    s.message_buttons
        .shortcuts
        .dialog
        .as_mut()?
        .calendar
        .as_mut()
}
fn editable(s: &ShellState, id: u64) -> bool {
    active(s, id).is_some_and(|c| c.stage == Stage::Editing && !c.fixture && !c.confirm_close)
}
fn context(s: &ShellState, id: u64) -> Option<Arc<Context>> {
    current(s, id).then(|| {
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .owner
            .ctx
            .clone()
    })
}
/// The Home button already supplied exact local Calendar intent, not an app
/// execution token. Reuse the same form, reads and retained uncertain attempts.
pub(super) fn begin_home(
    mut state: Signal<ShellState>,
    id: u64,
    provider: cal::Provider,
    preset: Option<String>,
) {
    let request = {
        let mut s = state.write();
        if !is_current(&s, id) {
            return;
        }
        let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
        if d.owner.home.is_none() || d.calendar.is_some() || d.phase != Phase::Choosing {
            return;
        }
        let ctx = d.owner.ctx.clone();
        let channel = d.owner.channel.clone();
        if cooling(&s, &ctx) {
            return;
        }
        let mut c = Calendar::new(
            provider,
            channel.clone(),
            load::initial_guests(&s, id),
            load::share_choices(&s, &ctx, "", &channel),
        );
        if let Some(date) = preset {
            c.date = date;
        }
        if let Some(old) = s
            .message_buttons
            .shortcuts
            .calendar_attempts
            .iter()
            .rev()
            .find(|a| {
                a.epoch == ctx.epoch
                    && a.team == ctx.session.team_id
                    && a.provider == provider
                    && !matches!(a.write, Write::Preference(_))
                    && matches!(a.result, Settlement::Sending | Settlement::Unknown)
            })
        {
            restore(&mut c, old, &s, &ctx);
            c.stage = Stage::Uncertain;
            c.notice=Some("An earlier calendar request has an unconfirmed outcome. It will not be sent again. Check your calendar before starting a different event.".into());
            c.attempt = Some(old.id);
        }
        let load = c.stage == Stage::Loading;
        s.message_buttons
            .shortcuts
            .dialog
            .as_mut()
            .unwrap()
            .calendar = Some(c);
        load.then_some(ctx)
    };
    if let Some(ctx) = request {
        load::start(state, id, ctx);
    }
}
/// Returns true only when the selected exact catalog action is this local flow.
pub(super) fn begin(mut state: Signal<ShellState>, id: u64) -> bool {
    let request = {
        let mut s = state.write();
        if !is_current(&s, id) {
            return false;
        }
        let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
        let Some(action) = d.selected.and_then(|i| d.catalog.as_ref()?.actions.get(i)) else {
            return false;
        };
        let Some(provider) = cal::Provider::for_action(action) else {
            return false;
        };
        if d.phase != Phase::Choosing || d.calendar.is_some() || d.fixture {
            return true;
        }
        let ctx = d.owner.ctx.clone();
        let channel = d.owner.channel.clone();
        if cooling(&s, &ctx) {
            return true;
        }
        let guests = load::initial_guests(&s, id);
        let choices = load::share_choices(&s, &ctx, "", &channel);
        let mut c = Calendar::new(provider, channel, guests, choices);
        if let Some(old) = s
            .message_buttons
            .shortcuts
            .calendar_attempts
            .iter()
            .rev()
            .find(|a| {
                a.epoch == ctx.epoch
                    && a.team == ctx.session.team_id
                    && a.provider == provider
                    && !matches!(a.write, Write::Preference(_))
                    && matches!(a.result, Settlement::Sending | Settlement::Unknown)
            })
        {
            restore(&mut c, old, &s, &ctx);
            c.stage = Stage::Uncertain;
            c.notice=Some("An earlier calendar request has an unconfirmed outcome. It will not be sent again. Check your calendar before starting a different event.".into());
            c.attempt = Some(old.id);
        }
        let load = c.stage == Stage::Loading;
        s.message_buttons
            .shortcuts
            .dialog
            .as_mut()
            .unwrap()
            .calendar = Some(c);
        load.then_some(ctx)
    };
    if let Some(ctx) = request {
        load::start(state, id, ctx);
    }
    true
}
/// Reopening an uncertain operation exposes its exact retained draft, without
/// refreshing it into a new write capability or substituting a newer account.
fn restore(c: &mut Calendar, a: &Attempt, s: &ShellState, ctx: &Context) {
    c.account = a.account.clone();
    match &a.write {
        Write::Event(e) => {
            c.title = e.title.clone();
            c.description = e.description.clone();
            c.start = Some(e.start);
            c.end = Some(e.end);
            c.date = cal::time::date_label(e.start);
            c.start_text = cal::time::label(e.start);
            c.end_text = cal::time::label(e.end);
            c.custom = true;
            c.calendar = e.calendar.clone();
            if !e.calendar.is_empty() {
                c.calendars.push(cal::Choice {
                    id: e.calendar.clone(),
                    name: e.calendar.clone(),
                });
            }
            c.guests = e
                .attendees
                .iter()
                .map(|id| {
                    load::person(s, ctx, id).unwrap_or_else(|| Person {
                        id: id.clone(),
                        label: id.clone(),
                    })
                })
                .collect();
            c.share = !e.share_channel.is_empty();
            if c.share {
                c.share_channel = e.share_channel.clone();
            }
            if !e.meeting_url.is_empty() {
                c.selected_room = "retained-conference".into();
                c.rooms
                    .insert(c.selected_room.clone(), e.meeting_url.clone());
                c.conferences.push(cal::Choice {
                    id: c.selected_room.clone(),
                    name: "Retained conference".into(),
                });
            }
        }
        Write::Room { app, .. } => {
            c.selected_room = app.clone();
            c.conferences.push(cal::Choice {
                id: app.clone(),
                name: app.clone(),
            });
            c.room_unknown = true;
        }
        Write::Preference(_) => {}
    }
}
fn close(s: &mut ShellState, id: u64, discard: bool) {
    let Some(d) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .filter(|d| d.id == id)
    else {
        return;
    };
    let Some(c) = d.calendar.as_ref() else {
        return;
    };
    if !discard && c.stage == Stage::Editing && c.dirty() {
        s.message_buttons
            .shortcuts
            .dialog
            .as_mut()
            .unwrap()
            .calendar
            .as_mut()
            .unwrap()
            .confirm_close = true;
        return;
    }
    // Dismissal never grants a write; captured attempts remain in the local book.
    cancel_owned(s, id);
}
fn external(s: &ShellState, id: u64, url: &str) {
    let Some(c) = active(s, id) else {
        return;
    };
    if c.auth_url.as_deref() == Some(url) || c.provider_link.as_deref() == Some(url) {
        if !c.fixture && cal::external_url(url) {
            let _ = crate::media::open_external(url);
        }
    }
}
#[cfg(test)]
mod tests;
