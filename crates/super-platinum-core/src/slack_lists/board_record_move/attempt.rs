use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardRecordMovePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
pub struct BoardRecordMoveAttempt {
    intent: BoardRecordMove,
    phase: BoardRecordMovePhase,
}
/// One consumed request-construction authority. No Clone or public constructor.
pub struct BoardRecordMovePermit {
    intent: BoardRecordMove,
}
impl BoardRecordMovePermit {
    pub(crate) fn into_intent(self) -> BoardRecordMove {
        self.intent
    }
}
impl BoardRecordMoveAttempt {
    pub fn new(intent: BoardRecordMove) -> Self {
        Self {
            intent,
            phase: BoardRecordMovePhase::Reviewing,
        }
    }
    pub fn intent(&self) -> &BoardRecordMove {
        &self.intent
    }
    pub fn phase(&self) -> BoardRecordMovePhase {
        self.phase
    }
    pub fn admit(
        &mut self,
        doc: &Document,
        page: &RecordsPage,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<BoardRecordMovePermit, ListError> {
        if self.phase != BoardRecordMovePhase::Reviewing {
            return Err(ListError("This Board move was already admitted."));
        }
        self.intent
            .revalidate(doc, page, context, allow, personal)?;
        self.phase = BoardRecordMovePhase::InFlight;
        Ok(BoardRecordMovePermit {
            intent: self.intent.clone(),
        })
    }
    /// Only a typed pre-dispatch refusal is NotRequested. ACKs and every uncertain
    /// post-dispatch outcome remain retained until an explicit current read.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != BoardRecordMovePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            BoardRecordMovePhase::NotRequested
        } else {
            BoardRecordMovePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, doc: &Document, page: &RecordsPage) -> bool {
        if self.phase != BoardRecordMovePhase::Unconfirmed
            || !self.intent.observes_desired(doc, page)
        {
            return false;
        }
        self.phase = BoardRecordMovePhase::Observed;
        true
    }
}
