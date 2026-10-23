//! Immutable canonical identities plus the actual companion/native selector bags.
use super::*;

pub(super) struct Source {
    canonical: Arc<Canonical>,
    members: Vec<peer::Candidate>,
    channels: Vec<Value>,
    value: Value,
}
impl Source {
    pub(super) fn capture(
        s: &ShellState,
        q: &Query,
        canonical: Arc<Canonical>,
        members: &[peer::Candidate],
        channels: &[Value],
    ) -> Result<Self, Error> {
        let value = observe(s, q, &canonical, members, channels)?;
        Ok(Self {
            canonical,
            members: members.to_vec(),
            channels: channels.to_vec(),
            value,
        })
    }
    pub(super) fn current(&self, s: &ShellState, q: &Query) -> bool {
        observe(s, q, &self.canonical, &self.members, &self.channels).is_ok_and(|v| v == self.value)
    }
    pub(super) fn value(&self) -> &Value {
        &self.value
    }
}

fn observe(
    s: &ShellState,
    q: &Query,
    canonical: &Canonical,
    members: &[peer::Candidate],
    channels: &[Value],
) -> Result<Value, Error> {
    let team = &q.ctx().session.team_id;
    if !q.current(s) {
        return Err(super::super::at("observations.source.query", unavailable()));
    }
    if !canonical.current_for(s, team) {
        return Err(super::super::at(
            "observations.source.canonical",
            unavailable(),
        ));
    }
    if members.iter().any(|m| !m.current(s)) {
        return Err(super::super::at(
            "observations.source.member-current",
            unavailable(),
        ));
    }
    let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
    let mut member_rows = BTreeMap::new();
    for m in members {
        let evidence = m.evidence();
        let raw = &evidence["source"];
        if raw["id"] != m.id()
            || !raw.is_object()
            || member_rows.insert(m.id().to_owned(), raw.clone()).is_some()
        {
            return Err(super::super::at(
                "observations.source.member-row",
                unavailable(),
            ));
        }
    }
    let mut ids = BTreeSet::new();
    for raw in channels {
        let id = raw["id"].as_str().ok_or_else(unavailable)?;
        if canonical.row(id) != Some(raw) || !ids.insert(id.to_owned()) {
            return Err(super::super::at(
                "observations.source.channel-row",
                unavailable(),
            ));
        }
    }
    let mut ims = member_rows
        .keys()
        .map(|id| (id.clone(), Value::Null))
        .collect::<BTreeMap<_, _>>();
    for raw in canonical.rows().iter().filter(|r| r["is_im"] == true) {
        let Some(peer) = raw["user"].as_str().filter(|id| ims.contains_key(*id)) else {
            continue;
        };
        if !ims[peer].is_null() {
            return Err(super::super::at(
                "observations.source.ambiguous-im",
                unavailable(),
            ));
        }
        let id = raw["id"].as_str().ok_or_else(unavailable)?;
        ids.insert(id.to_owned());
        ims.insert(peer.to_owned(), raw.clone());
    }
    let mut metadata = BTreeMap::new();
    let mut companions = BTreeMap::new();
    for id in ids {
        let channel = w
            .channels
            .get(&id)
            .ok_or_else(|| super::super::at("observations.source.native-channel", unavailable()))?;
        let bag = w.messages.get(&id);
        let key = super_platinum_core::drafts::DraftKey {
            team: team.clone(),
            channel: id.clone(),
            thread: None,
        };
        metadata.insert(id.clone(), json!({
            "latest":bag.and_then(|b|b.latest.as_ref()),
            "lastRead":bag.and_then(|b|b.last_read.as_ref()).or(channel.last_read.as_ref()),
            "draft":s.core.drafts.items.contains_key(&key) || w.draft_presence.has_channel(&id),
            "starred":channel.is_starred,
            "unread":bag.map(|b|b.unread_count > 0).unwrap_or_else(||channel.unread_count.is_some_and(|n|n>0)),
            "priority":w.priority_scores.get(&id)
        }));
        // Preserve the actual source of each companion, even when its effective
        // value equals a fallback. Transcript last() is never a source here.
        companions.insert(
            id,
            json!({"bagPresent":bag.is_some(),"latest":bag.and_then(|b|b.latest.as_ref()),
            "lastRead":bag.and_then(|b|b.last_read.as_ref()),"channelLastRead":channel.last_read,
            "unread":bag.map(|b|b.unread_count),"channelUnread":channel.unread_count,
            "draftPresence":w.draft_presence.has_channel(&key.channel),
            "localDraft":s.core.drafts.items.get(&key)}),
        );
    }
    Ok(json!({"members":member_rows,"ims":ims,"metadata":metadata,"companions":companions}))
}
