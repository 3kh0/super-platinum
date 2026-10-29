//! Sufficient participant matches from actual current local member source.
//! This adds acquisition candidates, never posting permission or remote authority.
use super::*;
use crate::directory_member_fetches::search as members;
use super_platinum_core::block_actions::home::workflow::directory_local as matching;
const MAX_QUERY_BYTES: usize = 64;
const MAX_MEMBERS: usize = 64;
const MAX_NAME_BYTES: usize = 4096;
#[derive(Clone)]
pub(super) struct Frame {
    source: Arc<members::Snapshot>,
    ids: BTreeSet<String>,
}
impl Frame {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.source.current(s)
    }
    pub(super) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}
fn peer(source: &Value) -> Option<Value> {
    let id = source.get("id")?.as_str()?;
    let mut fields = serde_json::Map::new();
    fields.insert("id".into(), Value::String(id.into()));
    for key in ["isUnknown", "isNonExistent", "is_self"] {
        if let Some(value) = source.get(key) {
            if !matches!(value, Value::Bool(_) | Value::Null) {
                return None;
            }
            fields.insert(key.into(), value.clone());
        }
    }
    for key in [
        "_real_name_normalized_lc",
        "_display_name_normalized_lc",
        "_email_normalized_lc",
    ] {
        if let Some(value) = source.get(key) {
            if !value.is_null() && !value.as_str().is_some_and(|v| v.len() <= MAX_NAME_BYTES) {
                return None;
            }
            fields.insert(key.into(), value.clone());
        }
    }
    Some(Value::Object(fields))
}
/// Called only after append's original all-ID/channel proof preflight. Fresh
/// LOCAL matching observation is not a replacement for the consumed channel Read.
pub(super) fn capture(
    s: &ShellState,
    ctx: &Context,
    rows: &HashMap<&str, &Value>,
    ids: &[String],
    query: &str,
    id: u64,
    generation: u64,
) -> Option<Frame> {
    // The retained normalizers are identity on this supported input domain.
    // No native locale approximation or remote-MPIM fuzzy passthrough is used.
    if query.is_empty()
        || query.len() > MAX_QUERY_BYTES
        || !query.bytes().all(|b| b.is_ascii_lowercase())
    {
        return None;
    }
    let w = s.core.workspaces.get(&ctx.session.team_id)?;
    let groups: Vec<_> = ids
        .iter()
        .filter_map(|id| {
            let native = w.channels.get(id)?;
            if load::share_channel_name(s, ctx, native)
                .to_lowercase()
                .contains(query)
            {
                return None;
            }
            let row = *rows.get(id.as_str())?;
            let members = row.get("members")?.as_array()?;
            if members.len() > MAX_MEMBERS
                || members.iter().any(|v| !v.as_str().is_some_and(valid_id))
            {
                return None;
            }
            Some((id, row, members))
        })
        .collect();
    if groups.is_empty() {
        return None;
    }
    let source = members::snapshot(
        s,
        &ctx.session.team_id,
        &format!("calendar-mpim-participants-{id}-{generation}"),
    )
    .ok()?;
    if !source.current(s) {
        return None;
    }
    let index: HashMap<_, _> = source
        .rows()
        .iter()
        .filter_map(|r| r.source["id"].as_str().map(|id| (id, &r.source)))
        .collect();
    let mut options = matching::Options::workflow(query, query, query, false);
    options.match_email = true;
    options.email_forward_only = true;
    let subqueries = vec![query.to_owned()];
    let remote = BTreeSet::new();
    let mut matched = BTreeSet::new();
    for (id, row, member_ids) in groups {
        let mut peers = BTreeMap::new();
        let mut complete = true;
        for member in member_ids {
            let member = member.as_str().unwrap();
            let Some(raw) = index.get(member).and_then(|raw| peer(raw)) else {
                complete = false;
                break;
            };
            peers.insert(member.to_owned(), raw);
        }
        if !complete {
            continue;
        }
        // Use the COMPLETE canonical membership after source union. Unknown/self
        // exclusions and the sum of ALL finite peer scores belong to the helper.
        let rows = [(*row).clone()];
        if !matching::filter_mpims(
            &rows,
            &peers,
            &options,
            &subqueries,
            &remote,
            |_| true,
            |_| None,
        )
        .is_empty()
        {
            matched.insert(id.clone());
        }
    }
    drop(index);
    (!matched.is_empty()).then(|| Frame {
        source: Arc::new(source),
        ids: matched,
    })
}
#[cfg(test)]
mod tests;
