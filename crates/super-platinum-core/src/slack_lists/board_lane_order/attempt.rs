use super::*;
/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct BoardLaneOrderAttempt {
    intent: BoardLaneOrder,
    phase: BoardLaneOrderPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardLaneOrderPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct BoardLaneOrderPermit {
    intent: BoardLaneOrder,
}
impl BoardLaneOrderPermit {
    pub(crate) fn into_intent(self) -> BoardLaneOrder {
        self.intent
    }
}
impl BoardLaneOrderAttempt {
    pub fn new(intent: BoardLaneOrder) -> Self {
        Self {
            intent,
            phase: BoardLaneOrderPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> BoardLaneOrderPhase {
        self.phase
    }
    pub fn intent(&self) -> &BoardLaneOrder {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<BoardLaneOrderPermit, ListError> {
        if self.phase != BoardLaneOrderPhase::Reviewing {
            return Err(ListError(
                "This lane order attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = BoardLaneOrderPhase::InFlight;
        Ok(BoardLaneOrderPermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the lane order.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != BoardLaneOrderPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            BoardLaneOrderPhase::NotRequested
        } else {
            BoardLaneOrderPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != BoardLaneOrderPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = BoardLaneOrderPhase::Observed;
        true
    }
}
