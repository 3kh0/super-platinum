//! Historical startup inventory; never rebuilds a Claim, Stream or clear token.
use super::*;
#[derive(Default)]
pub struct Inventory {
    drafts: Vec<editor::Draft>,
    claims: Vec<Record>,
    completed: Vec<cleanup::Receipt>,
}
impl Inventory {
    pub fn drafts(&self) -> &[editor::Draft] {
        &self.drafts
    }
    pub fn claims(&self) -> &[Record] {
        &self.claims
    }
    pub fn completed(&self) -> &[cleanup::Receipt] {
        &self.completed
    }
}
pub(super) fn read(conn: &Connection, account: &str) -> Result<Inventory, AppError> {
    let mut q=conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_hosted_format','composer_draft_hosted_records','composer_draft_hosted_drafts','composer_draft_hosted_completions') ORDER BY name")?;
    let names = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if names.is_empty() {
        return Ok(Inventory::default());
    }
    journal::require(conn, account)?;
    let editor = names.contains("composer_draft_hosted_drafts");
    let completion = names.contains("composer_draft_hosted_completions");
    if completion && !editor {
        return Err(invalid("orphan hosted completion inventory"));
    }
    let claims = journal::all(conn, account)?;
    let drafts = if editor {
        editor::all(conn, account)?
    } else {
        vec![]
    };
    let completed = if completion {
        cleanup::all(conn, account)?
    } else {
        vec![]
    };
    if claims.iter().any(|claim| {
        completed
            .iter()
            .any(|done| claim.id() == done.record().id())
    }) {
        return Err(invalid("hosted active/history collision"));
    }
    Ok(Inventory {
        drafts,
        claims,
        completed,
    })
}
