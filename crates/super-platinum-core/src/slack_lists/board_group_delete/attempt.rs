use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardGroupDeletePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
    NotRequired,
}
pub struct BoardGroupDeleteAttempt {
    intent: BoardGroupDelete,
    schema: BoardGroupDeletePhase,
    cells: BoardGroupDeletePhase,
}
/// Independent consumed authorities. Neither permit is Clone or publicly constructible.
pub struct BoardGroupDeleteSchemaPermit {
    intent: BoardGroupDelete,
}
pub struct BoardGroupDeleteCellsPermit {
    intent: BoardGroupDelete,
}
impl BoardGroupDeleteSchemaPermit {
    pub(crate) fn into_intent(self) -> BoardGroupDelete {
        self.intent
    }
}
impl BoardGroupDeleteCellsPermit {
    pub(crate) fn into_intent(self) -> BoardGroupDelete {
        self.intent
    }
}
impl BoardGroupDeleteAttempt {
    pub fn new(intent: BoardGroupDelete) -> Self {
        let cells = if intent.cells.is_empty() {
            BoardGroupDeletePhase::NotRequired
        } else {
            BoardGroupDeletePhase::Reviewing
        };
        Self {
            intent,
            schema: BoardGroupDeletePhase::Reviewing,
            cells,
        }
    }
    pub fn intent(&self) -> &BoardGroupDelete {
        &self.intent
    }
    pub fn schema_phase(&self) -> BoardGroupDeletePhase {
        self.schema
    }
    pub fn cells_phase(&self) -> BoardGroupDeletePhase {
        self.cells
    }
    pub fn admit(
        &mut self,
        doc: &Document,
        page: &RecordsPage,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<
        (
            BoardGroupDeleteSchemaPermit,
            Option<BoardGroupDeleteCellsPermit>,
        ),
        ListError,
    > {
        if self.schema != BoardGroupDeletePhase::Reviewing
            || !matches!(
                self.cells,
                BoardGroupDeletePhase::Reviewing | BoardGroupDeletePhase::NotRequired
            )
        {
            return Err(invalid());
        }
        self.intent
            .revalidate(doc, page, context, allow, personal)?;
        self.schema = BoardGroupDeletePhase::InFlight;
        let cells = if self.cells == BoardGroupDeletePhase::NotRequired {
            None
        } else {
            self.cells = BoardGroupDeletePhase::InFlight;
            Some(BoardGroupDeleteCellsPermit {
                intent: self.intent.clone(),
            })
        };
        Ok((
            BoardGroupDeleteSchemaPermit {
                intent: self.intent.clone(),
            },
            cells,
        ))
    }
    pub fn finish_schema(&mut self, not_requested: bool) -> bool {
        finish(&mut self.schema, not_requested)
    }
    pub fn finish_cells(&mut self, not_requested: bool) -> bool {
        finish(&mut self.cells, not_requested)
    }
    /// Each effect is observed independently; no write is retried or synthesized.
    pub fn observe(&mut self, doc: &Document, page: &RecordsPage) {
        if self.schema == BoardGroupDeletePhase::Unconfirmed && self.intent.observes_schema(doc) {
            self.schema = BoardGroupDeletePhase::Observed;
        }
        if self.cells == BoardGroupDeletePhase::Unconfirmed && self.intent.observes_cells(doc, page)
        {
            self.cells = BoardGroupDeletePhase::Observed;
        }
    }
}
fn finish(phase: &mut BoardGroupDeletePhase, not_requested: bool) -> bool {
    if *phase != BoardGroupDeletePhase::InFlight {
        return false;
    }
    *phase = if not_requested {
        BoardGroupDeletePhase::NotRequested
    } else {
        BoardGroupDeletePhase::Unconfirmed
    };
    true
}
