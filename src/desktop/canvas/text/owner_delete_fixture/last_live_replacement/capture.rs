use super::*;
use crate::app_home::inputs::external::fixture::capture as passive;
#[derive(Default)]
pub(super) struct Record {
    pub readiness: Value,
    pub stages: Vec<Value>,
    pub captures: serde_json::Map<String, Value>,
    pub viewport: Value,
}
pub(super) fn name() -> String {
    std::env::var("SUPER_PLATINUM_FIXTURE").unwrap()
}
impl Record {
    pub async fn ready(&mut self) -> Result<(), String> {
        self.readiness = passive::readiness(&name()).await?;
        Ok(())
    }
    pub async fn shot(
        &mut self,
        state: Signal<ShellState>,
        desktop: &dioxus::desktop::DesktopContext,
        stage: &str,
        mut geometry: Value,
        proof: Value,
    ) -> Result<(), String> {
        let stage = if confirmed::requested() && case() == "seed" && stage == "replaced" {
            "confirmed"
        } else {
            stage
        };
        let before = model(state).await?;
        let dom = observation(desktop).await?;
        self.viewport = geometry["viewport"].clone();
        passive::capture_image(desktop, &name(), stage, &mut geometry, &mut self.captures).await?;
        let after = model(state).await?;
        let dom_after = observation(desktop).await?;
        if before != after || dom != dom_after {
            return Err(format!(
                "Capture changed exact source/journal/DOM at {stage}: {}",
                json!({"before":before,"after":after,"domBefore":dom,"domAfter":dom_after})
            ));
        }
        self.stages.push(
            json!({"stage":stage,"ordinal":self.stages.len(),"geometry":geometry,
            "model":after,"dom":dom_after,"proof":proof}),
        );
        Ok(())
    }
    pub fn finish(self, result: Result<Value, String>) -> Value {
        let (ok, proof, error) = match result {
            Ok(p) => (true, p, None),
            Err(e) => (false, Value::Null, Some(e)),
        };
        json!({"ok":ok,"fixture":name(),"case":case(),"readiness":self.readiness,
            "viewport":self.viewport,"stages":self.stages,"captures":self.captures,
            "captureChecksPassed":ok,"networkCalls":0,"noSession":true,"noTransport":true,
            "proof":proof,"error":error})
    }
}
pub(super) async fn model(state: Signal<ShellState>) -> Result<Value, String> {
    let source = snapshot(&state.peek());
    let canonical = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Canonical observation worker")?;
    let journal = recovery::observe().await?;
    let shell = state.peek();
    if !enabled(&shell) {
        return Err("Fixture owner not disconnected".into());
    }
    let parent = shell.canvas_checklist.structure_fixture.as_ref().unwrap();
    let f = parent.owner_delete.as_ref().unwrap();
    let view = checklist::presentation(&shell).map(|v| json!({
        "visit":v.visit,"read":v.read,"loading":v.loading,"stale":v.stale,
        "operation":v.operation.map(|(serial,target,stage)|json!({"serial":serial,"target":target,"stage":format!("{stage:?}")})),
        "outstanding":v.outstanding_total,"error":v.error,"capacityReason":v.capacity_reason,
        "canonical":v.snapshot.as_deref().map(seed::saved)
    }));
    let mut value = json!({"canonical":canonical,"journal":journal,"writes":f.writes,"acknowledgements":f.acknowledgements,
        "pairReservations":parent.pair_reservations,"pairDispatches":parent.attempts,
        "reserved":parent.reserved_replacement.is_some(),"pendingPair":parent.pending_replacement.is_some(),
        "pendingText":f.pending_text.is_some(),"preflight":f.last_live.preflight,"completion":f.last_live.completion,
        "view":view,"account":shell.core.active_account,"accountEpoch":shell.core.account_epoch,
        "noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none()});
    if confirmed::requested() {
        value["restore"] = json!(f.last_live.restore);
    }
    if retirement_cancellation::requested() {
        value["retirement"] =
            crate::canvas::checklist::recovery::cancellation_fixture::proof(&shell);
    }
    Ok(value)
}
pub(super) async fn click(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<Value, String> {
    checked(desktop, &format!(r#"(()=>{{{}const e=document.querySelector({});if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});e.click();return{{ok:true,selector:{},visibleHitTested:true}};}})()"#,
        passive::ACTION_GUARD,json!(selector),json!(selector))).await
}
pub(super) async fn row(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focused: bool,
) -> Result<Value, String> {
    // Editor dismissal can precede the worker-produced mapping's DOM mount.
    // Wait for this exact canonical row before the unchanged geometry checks.
    wait(desktop, &format!(r#"({{ok:[...document.querySelectorAll('[data-canvas-text-section]')].some(e=>e.isConnected&&e.dataset.canvasTextSection==={})}})"#,json!(id))).await?;
    checked(desktop, &format!(r#"(()=>{{const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.dataset.canvasTextSection==={});if(!e)return{{ok:false}};e.scrollIntoView({{block:'nearest'}});return{{ok:true}};}})()"#,json!(id))).await?;
    check::row_actions(desktop, id, focused, true).await
}
pub(super) async fn open(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    deleting: bool,
) -> Result<Value, String> {
    row(desktop, id, false).await?;
    let selector = if deleting {
        "[data-structure-action=delete]"
    } else {
        ".canvas-text-open[aria-label='Edit paragraph']"
    };
    let action = click(
        desktop,
        &format!("[data-canvas-text-section={}] {selector}", json!(id)),
    )
    .await?;
    check::editor_ready(desktop, deleting).await?;
    if deleting {
        checked(desktop,r#"({ok:document.querySelector('.canvas-text-hint')?.textContent.includes('Delete this paragraph and leave an empty paragraph')&&document.querySelector('.canvas-text-hint')?.textContent.includes('both changes stay locked for review')})"#).await?;
    }
    Ok(action)
}
pub(super) async fn save(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    checked(desktop,&format!(r#"(()=>{{{}const e=document.getElementById('canvas-text-save');if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});e.click();e.click();return{{ok:true,visibleHitTested:true,clicks:2}};}})()"#,passive::ACTION_GUARD)).await
}
pub(super) async fn review_geometry(
    desktop: &dioxus::desktop::DesktopContext,
    controls: bool,
) -> Result<Value, String> {
    let selector = if controls {
        ".canvas-local-review .canvas-text-actions"
    } else {
        if confirmed::requested() {
            ".canvas-local-review"
        } else {
            ".canvas-local-review > .canvas-review-preview"
        }
    };
    checked(desktop,&format!(r#"(()=>{{const e=document.querySelector({}),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect();return{{ok:!!r&&!!c&&r.width>0&&r.height>0&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1,selector:{},text:e?.textContent,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#,json!(selector),json!(selector))).await
}

async fn observation(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    if retirement_cancellation::requested() {
        let script = format!(
            "(()=>{{if(!document.querySelector('.canvas-dialog'))return{{ok:true,closed:true,viewport:[innerWidth,innerHeight],active:{{id:document.activeElement?.id??null,tag:document.activeElement?.tagName??null}}}};return {};}})()",
            include_str!("observe.js")
        );
        checked(desktop, &script).await
    } else {
        checked(desktop, include_str!("observe.js")).await
    }
}
