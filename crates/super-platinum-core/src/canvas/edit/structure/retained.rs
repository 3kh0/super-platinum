//! Independent ordinary tombstones keep their exact identity and reserved position.
//! Split endpoints must continue through their existing, stricter pair validators.
use super::*;

pub(super) fn independent<'a>(
    rows: impl IntoIterator<Item = &'a Section>,
    target: &Section,
) -> Result<(), Error> {
    if target.crdt()?.is_some_and(|wire| wire.has(10)) {
        return Err(Error::Unsupported);
    }
    for row in rows {
        if let Some(wire) = row.crdt()? {
            for info in wire.messages(10)? {
                if info.string(3)?.ok_or(Error::Unsupported)? == target.id() {
                    return Err(Error::Unsupported);
                }
            }
        }
    }
    Ok(())
}
