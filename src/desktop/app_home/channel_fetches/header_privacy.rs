//! Original assignment-input observations for passive private-topic display.
//! This is native input custody, not Source state, row or action authority.
use super::{Context, Key, MemberFlags, Owner, members::Transformations};
use crate::{app_home::cfm_lifecycle, state::ShellState};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

// Native input bound, not a Source experiment-group schema assertion.
const MAX_GROUP_BYTES: usize = 128;

#[derive(Default)]
pub(super) struct Registry {
    inputs: BTreeMap<Key, Entry>,
}

struct Entry {
    context: Weak<Context>,
    nonce: Arc<()>,
    accepted: Option<Accepted>,
}

struct Accepted {
    input: Weak<()>,
    context: Weak<Context>,
    owner: Weak<()>,
    transformations: Weak<Transformations>,
    group: String,
}

fn same<T>(weak: &Weak<T>, strong: &Arc<T>) -> bool {
    Weak::ptr_eq(weak, &Arc::downgrade(strong))
}

fn context_current(context: &Context, shell: &ShellState) -> bool {
    context.current(shell) && shell.core.client.same_configuration(&context.client)
}

impl Accepted {
    fn matches(
        &self,
        input: &Arc<()>,
        context: &Arc<Context>,
        owner: &Arc<()>,
        transformations: &Arc<Transformations>,
    ) -> bool {
        same(&self.input, input)
            && same(&self.context, context)
            && same(&self.owner, owner)
            && same(&self.transformations, transformations)
    }
}

pub(super) struct Input {
    context: Arc<Context>,
    nonce: Arc<()>,
}

/// Call under the normal Signal write before the actual assignment IO. The
/// mutation retires old observations; that write also withdraws painted data.
/// No decoder, clone, consumer capture or default can create this input nonce.
pub(super) fn begin(shell: &mut ShellState, context: Arc<Context>) -> Option<Input> {
    if !context_current(&context, shell) {
        return None;
    }
    let nonce = Arc::new(());
    shell.app_home.channel_fetches.header_privacy.inputs.insert(
        context.key(),
        Entry {
            context: Arc::downgrade(&context),
            nonce: nonce.clone(),
            accepted: None,
        },
    );
    Some(Input { context, nonce })
}

impl Input {
    fn current(&self, shell: &ShellState) -> bool {
        context_current(&self.context, shell)
            && shell
                .app_home
                .channel_fetches
                .header_privacy
                .inputs
                .get(&self.context.key())
                .is_some_and(|entry| {
                    Arc::ptr_eq(&entry.nonce, &self.nonce) && same(&entry.context, &self.context)
                })
    }

    pub(super) fn observe(self, shell: &ShellState, raw: &Value) -> Option<Observation> {
        if !self.current(shell) || raw.get("ok") != Some(&Value::Bool(true)) {
            return None;
        }
        let group = raw
            .get("assignments")?
            .as_object()?
            .get("privacy_mode")?
            .as_object()?
            .get("group")?
            .as_str()
            .filter(|group| !group.is_empty() && group.len() <= MAX_GROUP_BYTES)?
            .to_owned();
        Some(Observation { input: self, group })
    }
}

pub(super) struct Observation {
    input: Input,
    group: String,
}

impl Observation {
    /// Bind only immediately after the same read's initial::begin, under the
    /// same Shell borrow. Its emoji make_mut work must already be finished.
    pub(super) fn bind(self, shell: &mut ShellState, owner: &Owner) {
        if !self.input.current(shell)
            || !Arc::ptr_eq(&self.input.context, &owner.context)
            || !owner.current(shell)
        {
            return;
        }
        let key = self.input.context.key();
        let Some(slot) = shell.app_home.channel_fetches.slots.get(&key) else {
            return;
        };
        if slot.initial.is_none()
            || !Arc::ptr_eq(&slot.context, &owner.context)
            || !Arc::ptr_eq(&slot.token, &owner.token)
        {
            return;
        }
        let accepted = Accepted {
            input: Arc::downgrade(&self.input.nonce),
            context: Arc::downgrade(&owner.context),
            owner: Arc::downgrade(&owner.token),
            transformations: Arc::downgrade(&slot.transformations),
            group: self.group,
        };
        let Some(entry) = shell
            .app_home
            .channel_fetches
            .header_privacy
            .inputs
            .get_mut(&key)
        else {
            return;
        };
        if Arc::ptr_eq(&entry.nonce, &self.input.nonce)
            && same(&entry.context, &self.input.context)
            && entry.accepted.is_none()
        {
            entry.accepted = Some(accepted);
        }
    }
}

/// Original data observation only. No clone/default/raw constructor, rows,
/// readiness or action permission. The consumer separately owns its topic.
pub(crate) struct Inactive {
    flags: MemberFlags,
    boot: cfm_lifecycle::PostBoot,
    context: Arc<Context>,
    owner: Arc<()>,
    input: Arc<()>,
    transformations: Weak<Transformations>,
}

impl Inactive {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !context_current(&self.context, shell)
            || !self.flags.current(shell)
            || !self.boot.current(shell)
        {
            return false;
        }
        let key = self.context.key();
        let Some(slot) = shell.app_home.channel_fetches.slots.get(&key) else {
            return false;
        };
        let Some(entry) = shell
            .app_home
            .channel_fetches
            .header_privacy
            .inputs
            .get(&key)
        else {
            return false;
        };
        let Some(accepted) = entry.accepted.as_ref() else {
            return false;
        };
        slot.initial.is_none()
            && Arc::ptr_eq(&slot.context, &self.context)
            && Arc::ptr_eq(&slot.token, &self.owner)
            && Arc::ptr_eq(&entry.nonce, &self.input)
            && same(&entry.context, &self.context)
            && same(&self.transformations, &slot.transformations)
            && accepted.group != "on"
            && accepted.matches(
                &self.input,
                &self.context,
                &self.owner,
                &slot.transformations,
            )
    }
}

/// Read existing accepted input data only; never allocate an input or slot.
pub(crate) fn inactive(shell: &ShellState, team: &str) -> Option<Inactive> {
    let key = Context::capture(shell, team).ok()?.key();
    let slot = shell.app_home.channel_fetches.slots.get(&key)?;
    let entry = shell
        .app_home
        .channel_fetches
        .header_privacy
        .inputs
        .get(&key)?;
    let accepted = entry.accepted.as_ref()?;
    if accepted.group == "on" {
        return None;
    }
    let observed = Inactive {
        flags: super::members::member_flags(shell, team).ok()?,
        boot: cfm_lifecycle::post_boot(shell, team).ok()?,
        context: slot.context.clone(),
        owner: slot.token.clone(),
        input: entry.nonce.clone(),
        transformations: accepted.transformations.clone(),
    };
    observed.current(shell).then_some(observed)
}
