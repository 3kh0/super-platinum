//! Once-per-operation canonical preparation; never retain this index over an await.
use super::*;
pub(in crate::message_buttons::shortcuts::calendar::sharing) struct Prepared<'a> {
    state: &'a State,
    current: bool,
    participants_current: bool,
    rows: HashMap<&'a str, &'a Value>,
}
pub(super) fn open(row: &Value, native: &Channel) -> bool {
    row["id"].as_str() == Some(native.id.as_str())
        && row.get("is_mpim") == Some(&Value::Bool(true))
        && row.get("is_open") == Some(&Value::Bool(true))
        && native.is_mpim
        && native.extra.get("is_open") == Some(&Value::Bool(true))
}
impl Prepared<'_> {
    fn canonical(&self, native: &Channel) -> bool {
        self.rows
            .get(native.id.as_str())
            .is_some_and(|row| open(row, native))
    }
    pub(in crate::message_buttons::shortcuts::calendar::sharing) fn cached(
        &self,
        s: &ShellState,
        ctx: &Context,
        native: &Channel,
        name: &str,
        selected: &str,
    ) -> bool {
        native.id == selected
            || !self.state.origins.guarded(&native.id)
            || (channel_base(s, ctx, native) && self.state.origins.independent(&native.id, name))
            || self.canonical(native)
    }
    pub(in crate::message_buttons::shortcuts::calendar::sharing) fn remote(
        &self,
        native: &Channel,
        generation: u64,
    ) -> bool {
        !self.state.origins.guarded(&native.id)
            || (self.state.generation == generation
                && self.state.hits.contains(&native.id)
                && self.canonical(native)
                && (self
                    .state
                    .participants
                    .as_ref()
                    .is_none_or(|frame| !frame.contains(&native.id))
                    || self.participants_current))
    }
    pub(in crate::message_buttons::shortcuts::calendar::sharing) fn unavailable(&self) -> bool {
        (!self.current && !self.state.hits.is_empty())
            || (self.state.participants.is_some() && !self.participants_current)
    }
}
pub(in crate::message_buttons::shortcuts::calendar::sharing) fn prepare<'a>(
    s: &'a ShellState,
    id: u64,
) -> Option<Prepared<'a>> {
    let c = active(s, id)?;
    let state = &c.mpims;
    let ctx = context(s, id)?;
    let facts = state
        .facts
        .as_ref()
        .filter(|v| v.current_for(s, &ctx.session.team_id));
    let current = facts.is_some();
    let rows = facts
        .map(|facts| {
            facts
                .rows()
                .iter()
                .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
                .collect()
        })
        .unwrap_or_default();
    let participants_current = state
        .participants
        .as_ref()
        .is_none_or(|frame| frame.current(s));
    Some(Prepared {
        state,
        current,
        participants_current,
        rows,
    })
}
