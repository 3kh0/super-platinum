//! Passive full state and capture fences over actual local/readback transitions.
use super::*;
pub(super) fn projection(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = crate::slack_lists::presentation(&shell).ok_or("No List presentation")?;
    let view = shell.slack_lists.view.as_ref().ok_or("No List view")?;
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.loading
        || p.stale
        || p.error.is_some()
        || !p.incomplete
    {
        return Err("Wrong disconnected projection owner".into());
    }
    Ok(
        json!({"lanes":p.lanes.iter().map(|lane|json!({"key":lane.key,"label":lane.label,"count":lane.count,"rows":lane.rows.iter().map(|r|&r.id).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "columns":p.columns.iter().map(|c|&c.id).collect::<Vec<_>>(),"matched":p.matched_rows,"pending":p.pending_refinement,"notice":p.view_notice,
        "selected":p.selected_view,"views":p.view_choices.iter().map(|v|json!([v.id,v.name])).collect::<Vec<_>>(),"expanded":view.expanded,
        "visit":view.generation,"selection":view.layout.selection_generation,"layout":view.layout.generation,"account":shell.core.active_account,"epoch":shell.core.account_epoch}),
    )
}
pub(super) struct Proof {
    pub mode: adapter::Case,
    pub original: Value,
    pub population: Value,
    pub find: Value,
    pub expanded: Value,
    pub stages: Vec<Value>,
    pub captures: serde_json::Map<String, Value>,
}
impl Proof {
    pub fn stable(
        &self,
        state: Signal<ShellState>,
        name: &str,
    ) -> Result<(Value, Value, Value), String> {
        let raw = adapter::state(&state.peek())?;
        let projected = projection(state)?;
        let ledger = adapter::snapshot(&state.peek())?;
        if raw["population"] != self.population
            || raw["find"] != self.find
            || projected["expanded"] != self.expanded
        {
            return Err("Raw page/cursor, Find or expansion changed".into());
        }
        oracle::state(self.mode, name, &raw, &projected, &ledger, &self.original)?;
        Ok((raw, projected, ledger))
    }
    pub async fn stage(
        &mut self,
        state: Signal<ShellState>,
        d: &dioxus::desktop::DesktopContext,
        name: &str,
    ) -> Result<(), String> {
        if oracle::stages(self.mode).get(self.stages.len()) != Some(&name) {
            return Err("Refinement capture order".into());
        }
        let before = self.stable(state, name)?;
        let geometry = wait_dom(
            d,
            &format!(
                "window.__listRefinementSave.geometry({}, {})",
                json!(name),
                json!(oracle::mode(self.mode))
            ),
        )
        .await?;
        let capture = harness::capture(d, name).await?;
        if self.stable(state, name)? != before {
            return Err("Refinement state/ledger changed during capture".into());
        }
        let geometry_after = checked(
            d,
            &format!(
                "window.__listRefinementSave.geometry({}, {})",
                json!(name),
                json!(oracle::mode(self.mode))
            ),
        )
        .await?;
        if self.stable(state, name)? != before {
            return Err("Refinement state/ledger changed after geometry".into());
        }
        self.stages.push(json!({"stage":name,"state":before.0,"projection":before.1,"ledger":before.2,"geometry":geometry,"geometryAfter":geometry_after}));
        self.captures.insert(name.into(), capture);
        Ok(())
    }
}
