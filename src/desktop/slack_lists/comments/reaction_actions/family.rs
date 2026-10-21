use super::*;
use super_platinum_core::state::ChannelMessages;

#[derive(Clone)]
pub(super) struct Family {
    pub parent: u64,
    pub base: String,
    pub selected: String,
    pub names: Vec<String>,
    shadow: Arc<Mutex<Shadow>>,
}
struct Shadow {
    message: Message,
    blocked: bool,
}
impl Family {
    pub fn new(
        parent: u64,
        base: String,
        selected: &str,
        names: Vec<String>,
        message: Message,
    ) -> Self {
        Self {
            parent,
            base,
            selected: selected.into(),
            names,
            shadow: Arc::new(Mutex::new(Shadow {
                message,
                blocked: false,
            })),
        }
    }
    pub fn current(&self, message: &Message) -> bool {
        self.shadow.lock().is_ok_and(|s| {
            !s.blocked
                && serde_json::to_value(&s.message).ok() == serde_json::to_value(message).ok()
        })
    }
    pub fn apply(&self, event: &RtEvent, socket: bool, actor: &str) {
        let (ts, user, name, adding) = match event {
            RtEvent::ReactionAdded {
                ts, user, reaction, ..
            } => (ts, user, reaction, true),
            RtEvent::ReactionRemoved {
                ts, user, reaction, ..
            } => (ts, user, reaction, false),
            _ => return,
        };
        let Ok(mut shadow) = self.shadow.lock() else {
            return;
        };
        if shadow.message.ts.as_ref() != Some(ts) {
            return;
        }
        // Never acquire new self-owned family children after the fixed plan.
        // A later opposite self add remains a refusal even if removed again.
        if socket
            && adding
            && user == actor
            && reaction_family_base(name) == Some(self.base.as_str())
            && (!self.names.contains(name) || !member(&shadow.message, user, name))
        {
            shadow.blocked = true;
        }
        let mut rows = ChannelMessages {
            messages: vec![shadow.message.clone()],
            ..Default::default()
        };
        rows.apply_reaction(ts, user, name, adding);
        shadow.message = rows.messages.remove(0);
    }
}
/// Additions require catalog/preferences; removals keep their accepted route.
pub(super) fn needs_addition(message: &Message, user: &str, selected: &str) -> bool {
    if !message.reactions.iter().any(|r| r.name == selected) {
        return false;
    }
    if reaction_family_base(selected).is_some() {
        own_reaction_family_names(message, user, selected).is_ok_and(|names| names.is_empty())
    } else {
        !selected.contains("::") && !member(message, user, selected)
    }
}
/// Plain singletons retain the accepted exact-action path. Known tone relations
/// use only fixed observed own names; unowned tone additions remain unavailable.
pub(super) fn selection(
    message: &Message,
    user: &str,
    selected: &str,
) -> Result<Option<(String, Vec<String>)>, ()> {
    let Some(base) = reaction_family_base(selected) else {
        return if selected.contains("::") {
            Err(())
        } else {
            Ok(None)
        };
    };
    let related = selected.contains("::")
        || message
            .reactions
            .iter()
            .any(|r| r.name.contains("::") && reaction_family_base(&r.name) == Some(base));
    if !related {
        return Ok(None);
    }
    let names = own_reaction_family_names(message, user, selected).map_err(|_| ())?;
    if names.is_empty() {
        return Err(());
    }
    Ok(Some((base.to_owned(), names)))
}
pub(super) fn prune(actions: &mut Vec<Attempt>, needed: usize) {
    if actions.len() + needed <= MAX_ACTIONS {
        return;
    }
    let retained: std::collections::BTreeSet<_> =
        actions.iter().filter(|a| a.blocking()).map(group).collect();
    actions.retain(|a| retained.contains(&group(a)));
}
pub(super) fn room(actions: &[Attempt], needed: usize) -> bool {
    if actions.len() + needed <= MAX_ACTIONS {
        return true;
    }
    let retained: std::collections::BTreeSet<_> =
        actions.iter().filter(|a| a.blocking()).map(group).collect();
    actions
        .iter()
        .filter(|a| retained.contains(&group(a)))
        .count()
        + needed
        <= MAX_ACTIONS
}
fn group(a: &Attempt) -> u64 {
    a.family.as_ref().map_or(a.id, |f| f.parent)
}
pub(super) fn socket(actions: &[Attempt], owner: &Owner, target: &Target, event: &RtEvent) {
    let mut seen = std::collections::BTreeSet::new();
    for a in actions
        .iter()
        .filter(|a| a.belongs(owner, target) && a.blocking())
    {
        if let Some(f) = &a.family {
            if seen.insert(f.parent) {
                f.apply(event, true, &a.user);
            }
        }
    }
}
pub(super) fn stop(work: &Work) {
    let Some(f) = &work.family else {
        return;
    };
    if let Ok(mut actions) = work.ledger.0.lock() {
        for a in actions
            .iter_mut()
            .filter(|a| group(a) == f.parent && matches!(a.phase, Phase::Queued | Phase::Preparing))
        {
            a.phase = Phase::NotSent;
            a.issued = Some(false);
        }
    }
}
pub(super) fn children(work: &Work, intents: &[ReactionIntent]) -> Option<Vec<Work>> {
    let f = work.family.as_ref()?;
    if intents.len() != f.names.len()
        || !intents
            .iter()
            .map(|i| i.record().name())
            .eq(f.names.iter().map(String::as_str))
    {
        return None;
    }
    let mut actions = work.ledger.0.lock().ok()?;
    let mut result = Vec::new();
    for (index, intent) in intents.iter().enumerate() {
        let a = actions
            .iter_mut()
            .find(|a| group(a) == f.parent && a.name == f.names[index])?;
        a.record = Some(intent.record().clone());
        a.phase = if index == 0 {
            Phase::Sent
        } else {
            Phase::Queued
        };
        let mut child = work.clone();
        child.id = a.id;
        child.name = a.name.clone();
        child.adding = false;
        result.push(child);
    }
    Some(result)
}
pub(super) fn status(actions: &[Attempt], f: &Family) -> String {
    let children: Vec<_> = actions.iter().filter(|a| group(a) == f.parent).collect();
    let count = |phase| children.iter().filter(|a| a.phase == phase).count();
    if children
        .iter()
        .any(|a| matches!(a.phase, Phase::Preparing | Phase::Queued | Phase::Sent))
    {
        return "Updating the reaction family. Your draft is unchanged.".into();
    }
    let parts = [
        (Phase::Confirmed, "confirmed"),
        (Phase::Desired, "already absent"),
        (Phase::Observed, "observed"),
        (Phase::Rejected, "rejected"),
        (Phase::Unknown, "unconfirmed"),
        (Phase::NotSent, "not sent"),
    ]
    .into_iter()
    .filter_map(|(p, label)| {
        let n = count(p);
        (n > 0).then(|| format!("{n} {label}"))
    })
    .collect::<Vec<_>>()
    .join(", ");
    let newer = children
        .iter()
        .any(|a| a.socket_observed || a.canonical_observed);
    format!(
        "Reaction family: {parts}.{}",
        if count(Phase::Unknown) > 0 {
            " Refresh comments to check delivery."
        } else if newer {
            " Newer comment updates are shown."
        } else {
            ""
        }
    )
}
