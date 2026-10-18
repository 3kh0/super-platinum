use super::{BoardChoiceChange, Document, ListError, PersonalTaskLists};
/// Process-local attempt state; not a durable dispatch ledger. The host retains
/// the original account/session/visit/read/revision and enforces no replay even
/// though PreparedRequest is cloneable. A new attempt requires a fresh review.
pub struct BoardChoiceAttempt {
    intent: BoardChoiceChange,
    phase: BoardChoicePhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardChoicePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// Consumed request-construction permit, deliberately not Clone.
pub struct BoardChoicePermit {
    intent: BoardChoiceChange,
}
impl BoardChoicePermit {
    pub(crate) fn into_intent(self) -> BoardChoiceChange {
        self.intent
    }
}
impl BoardChoiceAttempt {
    pub fn new(intent: BoardChoiceChange) -> Self {
        Self {
            intent,
            phase: BoardChoicePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> BoardChoicePhase {
        self.phase
    }
    pub fn intent(&self) -> &BoardChoiceChange {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<BoardChoicePermit, ListError> {
        if self.phase != BoardChoicePhase::Reviewing {
            return Err(ListError(
                "This Board group attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = BoardChoicePhase::InFlight;
        Ok(BoardChoicePermit {
            intent: self.intent.clone(),
        })
    }
    /// True is reserved for typed pre-dispatch NotRequested evidence. Every
    /// received response or post-dispatch failure requires a separate readback.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != BoardChoicePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            BoardChoicePhase::NotRequested
        } else {
            BoardChoicePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != BoardChoicePhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = BoardChoicePhase::Observed;
        true
    }
}
