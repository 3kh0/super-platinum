//! Passive supplied status glyphs in an ordinary current DM header.
//! Reads checked processed member pairs; never latest-wire or action authority.
use crate::{
    app_home::channel_fetches::canonical,
    directory_member_fetches::Context,
    state::{MainView, ShellState},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};
pub(super) mod clock;
pub(crate) use clock::Clock;
const GENERATION: &str = "conversation-header-supplied-status";
const FIXTURE_USER: &str = "UHEADERSTATUS";
const FIXTURE_CHANNEL: &str = "DHEADERSTATUS";
const FIXTURE_EMOJI: &str = ":coffee:";
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Provenance {
    Owned,
    Layout,
}
#[derive(Clone, PartialEq, Eq)]
struct Appearance {
    emoji: String,
    glyph: String,
    expires: u64,
}
struct Status {
    emoji: String,
    glyph: String,
    expires: u64,
    owner: Option<Arc<Owner>>,
    provenance: Provenance,
}
impl Status {
    fn appearance(&self) -> Appearance {
        Appearance {
            emoji: self.emoji.clone(),
            glyph: self.glyph.clone(),
            expires: self.expires,
        }
    }
}
// Guard-only original context. No Ready, Subset, row or action is retained.
struct Owner {
    context: Context,
    channel: String,
    user: String,
    generation: u64,
    view: MainView,
}
impl Owner {
    fn current(&self, s: &ShellState) -> bool {
        self.context.current(s)
            && self.context.client.same_configuration(&s.core.client)
            && std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none()
            && s.signed_in
            && !s.loading
            && s.core.screen == super_platinum_core::state::Screen::Main
            && s.detail_open()
            && s.main_view == self.view
            && s.channel_generation == self.generation
            && s.core.active_team.as_deref() == Some(self.context.session.team_id.as_str())
            && s.core.active_channel.as_deref() == Some(self.channel.as_str())
            && s.channels.get(s.active_channel).is_some_and(|v| {
                v.id == self.channel
                    && v.is_im
                    && !v.is_mpim
                    && v.user_id.as_deref() == Some(self.user.as_str())
            })
            && s.core
                .workspaces
                .get(&self.context.session.team_id)
                .and_then(|w| w.channels.get(&self.channel))
                .is_some_and(|c| {
                    c.is_im && !c.is_mpim && c.user.as_deref() == Some(self.user.as_str())
                })
    }
}
fn falsey_flag(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null | Value::Bool(false)))
}
fn ordinary_channel(row: &Value, id: &str, user: &str) -> bool {
    if row["id"].as_str() != Some(id) || row["is_im"] != true || row["user"].as_str() != Some(user)
    {
        return false;
    }
    for key in [
        "is_channel",
        "is_group",
        "is_mpim",
        "is_archived",
        "is_user_deleted",
        "is_file",
        "isUnknown",
        "isNonExistent",
        "is_deleted",
        "deleted",
    ] {
        if !falsey_flag(row.get(key)) {
            return false;
        }
    }
    let properties = row.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object()) {
        return false;
    }
    ["record_channel", "huddles", "meetings_channel"]
        .iter()
        .all(|key| {
            properties
                .and_then(|v| v.get(*key))
                .is_none_or(Value::is_null)
        })
}
fn ordinary_member(row: &Value, user: &super_platinum_core::slack::models::User) -> bool {
    row["id"].as_str() == Some(user.id.as_str())
        && !matches!(user.id.as_str(), "USLACKBOT" | "USLACK")
        && row.get("is_bot") == Some(&Value::Bool(false))
        && !user.is_bot
        && falsey_flag(row.get("is_slackbot"))
        && row.get("is_slackbot") == user.extra.get("is_slackbot")
        && row.get("deleted") == Some(&Value::Bool(false))
        && !user.deleted
}
fn no_alias(value: Option<&Value>) -> bool {
    // Literal absence/null in this checked processed JSON pair is not a fabricated
    // null or wire-casing normalization. Empty plain maps are a native adapter.
    value.is_none_or(|v| v.is_null() || v.as_object().is_some_and(|m| m.is_empty()))
}
fn supplied(
    profile: &Value,
    native: &super_platinum_core::slack::models::UserProfile,
    now: u64,
) -> Option<Appearance> {
    let profile = profile.as_object()?;
    let raw = profile
        .get("status_emoji")?
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 128)?;
    if native.status_emoji.as_deref() != Some(raw) {
        return None;
    }
    let expires = profile
        .get("status_expiration")?
        .as_u64()
        .filter(|v| *v <= MAX_SAFE_INTEGER)?;
    if native.status_expiration != i64::try_from(expires).ok() || (expires > 0 && expires <= now) {
        return None;
    }
    let info = profile.get("statusEmojiDisplayInfo");
    if info != native.extra.get("statusEmojiDisplayInfo") || !no_alias(info) {
        return None;
    }
    // Never silently normalize a differently cased wire field into this getter.
    if profile.get("status_emoji_display_info") != native.extra.get("status_emoji_display_info") {
        return None;
    }
    let name = raw.strip_prefix(':')?.strip_suffix(':')?;
    if !super_platinum_core::state::is_standard_emoji(name) {
        return None;
    }
    let glyph = super_platinum_core::state::emoji_glyph(name);
    if glyph.is_empty() {
        return None;
    }
    // Truthy supplied emoji makes BOTH Source synthesis predicates false even
    // with empty canonical text; unrelated status contributors are not borrowed.
    Some(Appearance {
        emoji: raw.to_owned(),
        glyph,
        expires,
    })
}
fn project(s: &ShellState, now: u64) -> Option<Status> {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        || !s.signed_in
        || s.loading
        || !s.detail_open()
        || s.core.screen != super_platinum_core::state::Screen::Main
    {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let channel = s.core.active_channel.as_deref()?;
    let vm = s.channels.get(s.active_channel)?;
    if vm.id != channel || !vm.is_im || vm.is_mpim {
        return None;
    }
    let w = s.core.workspaces.get(team)?;
    let native_channel = w.channels.get(channel)?;
    let user = native_channel.user.as_deref().filter(|v| !v.is_empty())?;
    if !native_channel.is_im
        || native_channel.is_mpim
        || native_channel.is_channel
        || native_channel.is_group
        || vm.user_id.as_deref() != Some(user)
    {
        return None;
    }
    let channels = canonical::observed(s, team).ok()?;
    if !channels.current_for(s, team) || !ordinary_channel(channels.row(channel)?, channel, user) {
        return None;
    }
    let context = Context::capture(s, team).ok()?;
    let members = w
        .directory_members
        .subset(
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
            &[user.to_owned()].into(),
            &w.users,
        )
        .ok()?;
    let member = members.rows().first()?;
    let native = w.users.get(user)?;
    if native.id != user
        || member.native["id"].as_str() != Some(user)
        || !ordinary_member(&member.source, native)
    {
        return None;
    }
    let appearance = supplied(member.source.get("profile")?, native.profile.as_ref()?, now)?;
    let owner = Arc::new(Owner {
        context,
        channel: channel.to_owned(),
        user: user.to_owned(),
        generation: s.channel_generation,
        view: s.main_view,
    });
    if !owner.current(s)
        || !channels.current_for(s, team)
        || !members.current(
            &w.directory_members,
            &owner.context.account,
            &owner.context.session,
            owner.context.epoch,
            GENERATION,
            &w.users,
        )
    {
        return None;
    }
    Some(Status {
        emoji: appearance.emoji,
        glyph: appearance.glyph,
        expires: appearance.expires,
        owner: Some(owner),
        provenance: Provenance::Owned,
    })
}
fn fixture_model(s: &ShellState) -> Option<Status> {
    if s.core.session.is_some() || s.core.transport.is_some() || s.core.active_account.is_some() {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let c = s.core.workspaces.get(team)?.channels.get(FIXTURE_CHANNEL)?;
    if s.core.active_channel.as_deref() != Some(FIXTURE_CHANNEL)
        || c.user.as_deref() != Some(FIXTURE_USER)
        || s.channels.get(s.active_channel)?.id != FIXTURE_CHANNEL
    {
        return None;
    }
    let profile = s
        .core
        .workspaces
        .get(team)?
        .users
        .get(FIXTURE_USER)?
        .profile
        .as_ref()?;
    if profile.status_emoji.as_deref() != Some(FIXTURE_EMOJI)
        || profile.status_expiration != Some(0)
    {
        return None;
    }
    Some(Status {
        emoji: FIXTURE_EMOJI.into(),
        glyph: super_platinum_core::state::emoji_glyph("coffee"),
        expires: 0,
        owner: None,
        provenance: Provenance::Layout,
    })
}
fn render_status(status: Status) -> Element {
    let provenance = if status.provenance == Provenance::Owned {
        "owned"
    } else {
        "layout"
    };
    let label = format!(
        "Status: {}",
        status.emoji.trim_matches(':').replace('_', " ")
    );
    rsx! {span{id:"conversation-header-status",class:"conversation-header-status",role:"img","aria-label":label,"data-status-provenance":provenance,"data-status-emoji":status.emoji,"{status.glyph}"}}
}
#[component]
pub(super) fn HeaderStatus(state: Signal<ShellState>) -> Element {
    let occurrence = use_hook(|| Rc::new(()));
    let cleanup = occurrence.clone();
    use_drop(move || clock::retire(&cleanup));
    let s = state.read();
    let status = match std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref() {
        Some("conversation-header-status" | "conversation-header-status-narrow") => {
            fixture_model(&s)
        }
        Some(_) => None,
        None => clock::now().and_then(|now| project(&s, now)),
    };
    clock::display(occurrence, status.as_ref());
    status.map(render_status).unwrap_or_else(|| rsx! {})
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = None;
    s.overlay = None;
    if let Some(team) = s.core.active_team.clone() {
        if let Some(w) = s.core.workspaces.get_mut(&team) {
            let channel = serde_json::from_value(
                json!({"id":FIXTURE_CHANNEL,"is_im":true,"user":FIXTURE_USER}),
            )
            .unwrap();
            let user=serde_json::from_value(json!({"id":FIXTURE_USER,"name":"riley","profile":{"display_name":"Riley Park","status_text":"Coffee break","status_emoji":FIXTURE_EMOJI,"status_expiration":0}})).unwrap();
            w.channels.insert(FIXTURE_CHANNEL.into(), channel);
            w.users.insert(FIXTURE_USER.into(), user);
            s.core.active_channel = Some(FIXTURE_CHANNEL.into());
        }
    }
    s.refresh_from_core();
    if let Some(index) = s.channels.iter().position(|c| c.id == FIXTURE_CHANNEL) {
        s.select_channel(index, crate::state::ChannelOpen::Global);
    }
}
#[cfg(test)]
mod tests;
