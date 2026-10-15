use super::{control, fixture, ledger, native};
use crate::state::{ChannelOpen, ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) struct Actions {
    pub account: u64,
    pub process: String,
    pub next: Option<Box<ShellState>>,
    pub held: u64,
    pub event: Option<native::Event>,
    pub targets: BTreeMap<String, u64>,
}
pub(super) fn info(shell: &ShellState) -> Value {
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "channel":shell.core.active_channel,"thread":shell.thread_root,
        "channelScope":crate::view::composer::field_scope(shell,ComposerTarget::Channel),
        "threadScope":crate::view::composer::field_scope(shell,ComposerTarget::Thread),
        "catalog":crate::emoji_catalog::retirement_observation(shell),
        "isolated":shell.core.session.is_none()&&shell.core.transport.is_none()&&shell.core.cache.is_none(),
        "team":shell.core.active_team,
        "prefs":shell.core.workspaces.get("T1").map(|w|serde_json::json!({"loaded":w.emoji_preferences.loaded(),"revision":w.emoji_preferences.revision()}))})
}
fn ready_generation(shell: &ShellState) -> Result<u64, String> {
    let observation = crate::emoji_catalog::retirement_observation(shell);
    if observation["ready"]["current"] != true {
        return Err("No current target generation".into());
    }
    observation["ready"]["generation"]
        .as_u64()
        .filter(|g| *g != 0)
        .ok_or("Missing observed generation".into())
}
async fn ready(state: Signal<ShellState>, account: u64, revision: u64) -> Result<(), String> {
    loop {
        let shell = state.peek();
        if !fixture::owner(&shell, account) {
            return Err("Ready wait owner retired".into());
        }
        let observation = crate::emoji_catalog::retirement_observation(&shell);
        if observation["ready"]["current"] == true && observation["ready"]["revision"] == revision {
            return Ok(());
        }
        drop(shell);
        control::changed().await;
    }
}
impl Actions {
    pub fn bind(&mut self, key: String, generation: u64) -> Result<(), String> {
        if generation == 0 || self.targets.contains_key(&key) {
            return Err("Duplicate/zero target binding".into());
        }
        let id = ledger::scalar_id(&key);
        ledger::emit(ledger::event::TARGET, generation, 1, id);
        ledger::emit(ledger::event::TARGET, generation, 2, id);
        self.targets.insert(key, generation);
        Ok(())
    }
    pub fn role(action: &Value) -> Option<String> {
        let id = action["id"].as_str()?;
        action["subcase"]
            .as_str()
            .map(|_| id.rsplit_once('/').unwrap().0.to_owned())
    }
    pub async fn execute(
        &mut self,
        mut state: Signal<ShellState>,
        action: &Value,
    ) -> Result<Value, String> {
        if !fixture::owner(&state.peek(), self.account) {
            return Err("Action owner refused".into());
        }
        let id = action["id"].as_str().ok_or("Missing action id")?;
        let verb = id.rsplit('/').next().unwrap();
        let sub = action["subcase"].as_str().unwrap_or("");
        let role = Self::role(action);
        let mut generation = 0;
        match (sub, verb) {
            ("refresh.closed", "refresh") => {
                self.bind(role.unwrap(), ready_generation(&state.peek())?)?;
                fixture::mutate(&mut state.write(), self.account, 2)?;
                drop(crate::emoji_catalog::ensure(state));
                ready(state, self.account, 2).await?;
            }
            ("refresh.picker", "refresh") => {
                fixture::mutate(&mut state.write(), self.account, 3)?;
                // The existing picker effect owns its ensure. No second ensure.
                ready(state, self.account, 3).await?;
            }
            ("completion.channel", "navigate") | ("", "thread-channel") => {
                let destination = if verb == "navigate" { "C1" } else { "C2" };
                let mut shell = state.write();
                let index = shell
                    .channels
                    .iter()
                    .position(|c| c.id == destination)
                    .ok_or("Fixture channel missing")?;
                shell.select_channel(index, ChannelOpen::Global);
            }
            ("", "thread-open") => {
                crate::bootstrap::open_thread(state, "C2".into(), "1719800000.000100".into()).await
            }
            ("completion.thread", "close") => state.write().close_thread(),
            ("reject.local", "start") => {
                control::arm(self.account, 4, false, None)?;
                fixture::mutate(&mut state.write(), self.account, 4)?;
                drop(crate::emoji_catalog::ensure(state));
                generation = control::reached().await?;
                self.held = generation;
                self.bind(role.unwrap(), generation)?;
            }
            ("reject.local", "invalidate") => fixture::mutate(&mut state.write(), self.account, 5)?,
            ("reject.remote", "start") => {
                let rows = tokio::task::spawn_blocking(fixture::remote_response)
                    .await
                    .map_err(|_| "Remote fixture worker failed")?;
                if !fixture::owner(&state.peek(), self.account) {
                    return Err("Remote arm owner retired".into());
                }
                control::arm(self.account, 5, true, None)?;
                control::arm_remote(self.account, rows)?;
                drop(crate::emoji_catalog::ensure(state));
                generation = control::reached().await?;
                self.held = generation;
                self.bind(role.unwrap(), generation)?;
            }
            ("reject.remote", "invalidate") => {
                fixture::mutate(&mut state.write(), self.account, 6)?
            }
            ("reject.local" | "reject.remote", "release") => {
                control::release()?;
                generation = self.held;
                while !control::rejected(generation) {
                    control::changed().await;
                }
                let observed = crate::emoji_catalog::retirement_observation(&state.peek());
                let old_ticket = (generation & ((1u64 << 48) - 1)) >> 2;
                if observed["building"]["ticket"] == old_ticket {
                    return Err("Rejected building ticket remained".into());
                }
            }
            ("", "recover-local" | "recover-remote") => {
                let revision = if verb == "recover-local" { 5 } else { 6 };
                drop(crate::emoji_catalog::ensure(state));
                ready(state, self.account, revision).await?;
            }
            ("", "replacement-ready") => {
                if self.next.is_some() {
                    return Err("Duplicate replacement preparation".into());
                }
                let media = state.peek().media.clone();
                let account = self.account + 1;
                self.next = Some(
                    tokio::task::spawn_blocking(move || {
                        let mut shell = Box::new(ShellState::fixture(media));
                        fixture::install(&mut shell, account);
                        shell
                    })
                    .await
                    .map_err(|_| "Replacement worker failed")?,
                );
            }
            ("account.reset", "replace") => {
                let next = self.next.take().ok_or("Replacement was not prepared")?;
                self.account += 1;
                if !fixture::owner(&next, self.account) {
                    return Err("Replacement owner mismatch".into());
                }
                crate::bootstrap::install_loaded(state, *next);
            }
            ("", "catalog-start") => {
                let event = self
                    .event
                    .take()
                    .ok_or("Missing shutdown event capability")?;
                control::arm(self.account, 2, false, Some(event))?;
                fixture::mutate(&mut state.write(), self.account, 2)?;
                drop(crate::emoji_catalog::ensure(state));
                generation = control::reached().await?;
                self.held = generation;
                self.bind(format!("{}/shutdown", self.process), generation)?;
            }
            ("refresh.picker" | "account.reset", "open")
            | ("completion.channel" | "completion.thread" | "account.reset", "query")
            | ("completion.channel" | "completion.thread", "dismiss")
            | ("refresh.picker", "close") => {}
            (
                "",
                "boot-ready" | "seed-picker-open" | "seed-picker-close" | "account-clear" | "query"
                | "dismiss",
            ) => {}
            _ => return Err(format!("Unknown native operation: {sub}/{verb}")),
        }
        if !fixture::owner(&state.peek(), self.account) {
            return Err("Action completion owner retired".into());
        }
        Ok(
            json!({"ok":true,"id":id,"nativeComplete":true,"generation":generation,"state":info(&state.peek())}),
        )
    }
    pub fn accepted(
        &mut self,
        state: Signal<ShellState>,
        action: &Value,
        result: &Value,
    ) -> Result<(), String> {
        let id = action["id"].as_str().ok_or("Missing id")?;
        let verb = id.rsplit('/').next().unwrap();
        let sub = action["subcase"].as_str().unwrap_or("");
        if matches!(
            (sub, verb),
            ("refresh.picker", "open")
                | ("completion.channel" | "completion.thread", "query")
                | ("account.reset", "open")
        ) {
            let observation = crate::emoji_catalog::retirement_observation(&state.peek());
            if result["accepted"]["catalog"] != observation["ready"]["token"] {
                return Err("Accepted token changed before binding".into());
            }
            self.bind(
                Self::role(action).unwrap(),
                ready_generation(&state.peek())?,
            )?;
        }
        if sub.is_empty()
            && id.contains("/setup/shutdown/")
            && verb == "query"
            && self.process.starts_with('W')
        {
            self.bind(
                format!("{}/shutdown", self.process),
                ready_generation(&state.peek())?,
            )?;
        }
        Ok(())
    }
}
