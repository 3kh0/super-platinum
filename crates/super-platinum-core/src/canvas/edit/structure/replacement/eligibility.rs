//! Borrowed full-inventory classification shared by presentation and v2 recovery.
//! Callers first validate every retained tombstone through its history/lineage proof.
use super::*;

pub(in crate::canvas::edit::structure) fn last_live<'a>(
    rows: &[&'a Section],
) -> Result<&'a Section, Error> {
    let mut title = None;
    let mut target = None;
    let mut first = None;
    for &row in rows {
        let pos = row.raw().string(8)?.ok_or(Error::Unsupported)?;
        position::canonical(pos)?;
        if first.is_none_or(|previous: &str| pos < previous) {
            first = Some(pos);
        }
        if row.parents()?.is_some_and(|p| !p.fields().is_empty()) {
            return Err(Error::Unsupported);
        }
        if row.deleted()? == Some(true) {
            continue;
        }
        if row.section_type()? == Some(64) && row.style()? == Some(48) {
            if title.replace(row).is_some() {
                return Err(Error::Unsupported);
            }
        } else if row.section_type()?.unwrap_or(0) == 0
            && row.style()?.unwrap_or(0) == 0
            && row.read_only()? != Some(true)
            && !row.attributes()?.is_some_and(|a| a.has(29))
        {
            if target.replace(row).is_some() {
                return Err(Error::Unsupported);
            }
        } else {
            return Err(Error::Unsupported);
        }
    }
    let title = title.ok_or(Error::Unsupported)?;
    let target = target.ok_or(Error::Unsupported)?;
    if title.raw().string(8)? != first {
        return Err(Error::Unsupported);
    }
    retained::independent(rows.iter().copied(), target)?;
    Ok(target)
}
