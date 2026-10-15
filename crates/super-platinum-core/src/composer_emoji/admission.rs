//! One-use authorization for native emoji replacement. No method mutates a
//! composer. The renderer must perform its final DOM check, insert atomically,
//! then acknowledge success before sending the next completion query.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Snapshot {
    pub text: String,
    pub anchor: usize,
    pub focus: usize,
    pub direction: String,
}

impl Snapshot {
    pub fn byte_selection(&self) -> Option<(usize, usize)> {
        if !matches!(self.direction.as_str(), "none" | "forward" | "backward") {
            return None;
        }
        Some((
            utf16_boundary(&self.text, self.anchor)?,
            utf16_boundary(&self.text, self.focus)?,
        ))
    }
}

fn utf16_boundary(text: &str, offset: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units == offset {
            return Some(byte);
        }
        units += c.len_utf16();
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(text.len())
}

#[derive(Clone, Debug)]
pub struct Choice {
    /// Unique row identity, including the matched canonical alias. Frecency IDs
    /// alone are not sufficient: aliases may share the same base Unicode ID.
    pub id: String,
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub spacer: String,
    pub usage_id: String,
    pub query: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ticket {
    pub serial: u64,
    pub seq: u64,
    pub owner: String,
    pub catalog: String,
    pub choice: String,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct Answer {
    pub kind: &'static str,
    pub serial: u64,
    pub seq: u64,
    pub owner: String,
    pub catalog: String,
    pub choice: String,
    pub allowed: bool,
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub spacer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub id: String,
    pub query: String,
}

struct Presented<C> {
    seq: u64,
    snapshot: Snapshot,
    catalog: Arc<C>,
    catalog_token: String,
    preferences_revision: u64,
    choices: Vec<Choice>,
}

struct Pending {
    ticket: Ticket,
    after: Snapshot,
    usage: Usage,
}

pub struct Gate<C> {
    owner: String,
    last_serial: u64,
    presented: Option<Presented<C>>,
    pending: Option<Pending>,
}

impl<C> Gate<C> {
    pub fn new(owner: String) -> Self {
        Self {
            owner,
            last_serial: 0,
            presented: None,
            pending: None,
        }
    }

    pub fn dismiss(&mut self) {
        self.presented = None;
        self.pending = None;
    }

    /// Called only for a result actually sent for this captured query. Catalog
    /// allocation identity is retained in addition to its wire generation token.
    pub fn present(
        &mut self,
        seq: u64,
        snapshot: Snapshot,
        catalog: Arc<C>,
        catalog_token: String,
        preferences_revision: u64,
        choices: Vec<Choice>,
    ) {
        self.dismiss();
        self.presented = Some(Presented {
            seq,
            snapshot,
            catalog,
            catalog_token,
            preferences_revision,
            choices,
        });
    }

    pub fn admit(
        &mut self,
        ticket: Ticket,
        current_owner: &str,
        current_catalog: &Arc<C>,
        preferences_revision: u64,
    ) -> Option<Answer> {
        if ticket.serial <= self.last_serial {
            return None;
        }
        self.last_serial = ticket.serial;
        self.pending = None;
        let presented = self.presented.as_ref()?;
        if current_owner != self.owner
            || ticket.owner != self.owner
            || ticket.seq != presented.seq
            || ticket.snapshot != presented.snapshot
            || ticket.catalog != presented.catalog_token
            || !Arc::ptr_eq(current_catalog, &presented.catalog)
            || preferences_revision != presented.preferences_revision
        {
            return None;
        }
        let choice = presented.choices.iter().find(|c| c.id == ticket.choice)?;
        let (anchor, focus) = ticket.snapshot.byte_selection()?;
        if anchor != focus
            || choice.end != ticket.snapshot.focus
            || choice.start > choice.end
            || choice.value.is_empty()
            || !matches!(choice.spacer.as_str(), "" | " ")
        {
            return None;
        }
        let start = utf16_boundary(&ticket.snapshot.text, choice.start)?;
        let after_text = format!(
            "{}{}{}{}",
            &ticket.snapshot.text[..start],
            choice.value,
            choice.spacer,
            &ticket.snapshot.text[focus..]
        );
        let caret = choice.start + choice.value.encode_utf16().count() + choice.spacer.len();
        let after = Snapshot {
            text: after_text,
            anchor: caret,
            focus: caret,
            direction: "forward".into(),
        };
        let answer = Answer {
            kind: "emoji_admit",
            serial: ticket.serial,
            seq: ticket.seq,
            owner: ticket.owner.clone(),
            catalog: ticket.catalog.clone(),
            choice: ticket.choice.clone(),
            allowed: true,
            value: choice.value.clone(),
            start: choice.start,
            end: choice.end,
            spacer: choice.spacer.clone(),
        };
        self.pending = Some(Pending {
            ticket,
            after,
            usage: Usage {
                id: choice.usage_id.clone(),
                query: choice.query.clone(),
            },
        });
        Some(answer)
    }

    /// Accept at most one actual native success. A query/dismiss/owner/catalog
    /// change retires the authorization. No optimistic usage is recorded for a
    /// refused DOM admission or failed native edit.
    pub fn inserted(
        &mut self,
        ticket: &Ticket,
        after: &Snapshot,
        current_owner: &str,
        current_catalog: &Arc<C>,
        preferences_revision: u64,
    ) -> Option<Usage> {
        let pending = self.pending.as_ref()?;
        if pending.ticket.serial != ticket.serial {
            return None;
        }
        let pending = self.pending.take()?;
        let presented = self.presented.as_ref()?;
        if ticket.seq != pending.ticket.seq
            || ticket.owner != pending.ticket.owner
            || ticket.catalog != pending.ticket.catalog
            || ticket.choice != pending.ticket.choice
            || ticket.snapshot != pending.ticket.snapshot
            || after != &pending.after
            || current_owner != self.owner
            || !Arc::ptr_eq(current_catalog, &presented.catalog)
            || preferences_revision != presented.preferences_revision
        {
            return None;
        }
        self.presented = None;
        Some(pending.usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Gate<()>, Arc<()>, Ticket) {
        let catalog = Arc::new(());
        let snapshot = Snapshot {
            text: "aa :sm".into(),
            anchor: 6,
            focus: 6,
            direction: "none".into(),
        };
        let ticket = Ticket {
            serial: 1,
            seq: 4,
            owner: "mount/team/channel".into(),
            catalog: "catalog-7".into(),
            choice: "E1f604/smile".into(),
            snapshot: snapshot.clone(),
        };
        let mut gate = Gate::new(ticket.owner.clone());
        gate.present(
            4,
            snapshot,
            catalog.clone(),
            "catalog-7".into(),
            2,
            vec![Choice {
                id: ticket.choice.clone(),
                value: "\u{1f604}".into(),
                start: 3,
                end: 6,
                spacer: " ".into(),
                usage_id: "E1f604".into(),
                query: "sm".into(),
            }],
        );
        (gate, catalog, ticket)
    }
    #[test]
    fn full_text_direction_owner_allocation_and_preferences_guard_admission() {
        for alteration in 0..5 {
            let (mut gate, catalog, mut ticket) = setup();
            let mut owner = ticket.owner.clone();
            let mut current = catalog;
            let mut revision = 2;
            match alteration {
                0 => ticket.snapshot.text = "bb :sm".into(),
                1 => ticket.snapshot.direction = "backward".into(),
                2 => owner = "another-mount".into(),
                3 => current = Arc::new(()),
                _ => revision += 1,
            }
            assert!(gate.admit(ticket, &owner, &current, revision).is_none());
        }
    }
    #[test]
    fn admission_does_not_record_usage_and_native_ack_is_exact_and_one_use() {
        let (mut gate, catalog, ticket) = setup();
        assert!(
            gate.admit(ticket.clone(), &ticket.owner, &catalog, 2)
                .is_some()
        );
        let after = Snapshot {
            text: "aa \u{1f604} ".into(),
            anchor: 6,
            focus: 6,
            direction: "forward".into(),
        };
        assert_eq!(
            gate.inserted(&ticket, &after, &ticket.owner, &catalog, 2),
            Some(Usage {
                id: "E1f604".into(),
                query: "sm".into()
            })
        );
        assert!(
            gate.inserted(&ticket, &after, &ticket.owner, &catalog, 2)
                .is_none()
        );
        assert!(
            gate.admit(ticket.clone(), &ticket.owner, &catalog, 2)
                .is_none()
        );
    }
    #[test]
    fn dismiss_and_failed_ack_never_record_and_surrogate_halves_are_invalid() {
        let (mut gate, catalog, ticket) = setup();
        assert!(
            gate.admit(ticket.clone(), &ticket.owner, &catalog, 2)
                .is_some()
        );
        assert!(
            gate.inserted(&ticket, &ticket.snapshot, &ticket.owner, &catalog, 2)
                .is_none()
        );
        gate.dismiss();
        assert!(
            gate.admit(
                Ticket {
                    serial: 2,
                    ..ticket.clone()
                },
                &ticket.owner,
                &catalog,
                2
            )
            .is_none()
        );
        assert_eq!(utf16_boundary("\u{1f604}", 1), None);
        assert_eq!(utf16_boundary("\u{1f604}", 2), Some(4));
    }
    #[test]
    fn native_ack_requires_explicit_forward_without_relaxing_before_direction() {
        for direction in ["none", "backward"] {
            let (mut gate, catalog, ticket) = setup();
            assert!(
                gate.admit(ticket.clone(), &ticket.owner, &catalog, 2)
                    .is_some()
            );
            let after = Snapshot {
                text: "aa \u{1f604} ".into(),
                anchor: 6,
                focus: 6,
                direction: direction.into(),
            };
            assert!(
                gate.inserted(&ticket, &after, &ticket.owner, &catalog, 2)
                    .is_none()
            );
        }
    }
}
