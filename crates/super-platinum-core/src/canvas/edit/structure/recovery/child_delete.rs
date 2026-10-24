//! V1 retains enough source/request custody to validate dependent child Delete.
use super::*;
use automerge::ReadDoc;

impl Checkpoint {
    /// Source/request custody only: missing observed peers never confirm success.
    pub(super) fn is_dependent_child_delete(&self) -> Result<bool, Error> {
        self.validate()?;
        if !self.is_delete() {
            return Ok(false);
        }
        let rows = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        let target = rows
            .iter()
            .find(|r| r.id() == self.target)
            .ok_or(Error::Unsupported)?;
        if target.crdt()?.is_some_and(|w| w.has(10)) {
            return Ok(false);
        }
        Ok(super::super::split::child_delete_owner(&rows, target, 0)?.is_some())
    }

    pub(super) fn validate_child_delete(&self) -> Result<(), Error> {
        if !self.is_delete() {
            return Ok(());
        }
        let rows = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        let child = rows
            .iter()
            .find(|r| r.id() == self.target)
            .ok_or(Error::Unsupported)?;
        if super::super::split::child_delete_owner(&rows, child, 0)?.is_none() {
            return Ok(());
        }
        let draft = text::canonical_draft(child)?;
        if draft.rtml()? != self.draft {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        let update = data.message(1)?.ok_or(Error::Unsupported)?;
        let wire = update.message(38)?.ok_or(Error::Unsupported)?;
        super::super::split::child_delete_transition(child, &wire, child)?;
        let expected = RawMessage::from_fields(&[
            F::bytes(1, self.target.as_bytes())?,
            F::uint(2, child.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(3, 1)?,
            F::uint(9, 0)?,
            F::message(13, &RawMessage::decode(&[])?)?,
            F::uint(19, 1)?,
            F::uint(20, 1)?,
            F::bytes(33, format!("{}-0", self.session).as_bytes())?,
            F::message(38, &wire)?,
        ])?;
        if update != expected || wire.has(4) || wire.has(7) {
            return Err(Error::Unsupported);
        }
        let source = child.crdt()?.ok_or(Error::Unsupported)?;
        let mut a = source.clone();
        let mut b = wire.clone();
        for tag in [4, 5, 6, 7, 12] {
            a = a.remove(tag)?;
            b = b.remove(tag)?;
        }
        if a != b {
            return Err(Error::Unsupported);
        }
        let doc = automerge::Automerge::load(wire.bytes(5)?.ok_or(Error::Unsupported)?)
            .map_err(|_| Error::Unsupported)?;
        let heads = doc.get_heads();
        let [head] = heads.as_slice() else {
            return Err(Error::Unsupported);
        };
        let change = doc.get_change_by_hash(head).ok_or(Error::Unsupported)?;
        if change.actor_id().to_hex_string() != format!("{}00000000", self.session)
            || wire.string(12)? != Some(head.to_string().as_str())
        {
            return Err(Error::Unsupported);
        }
        let old = source
            .fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| Ok(source.encoded_field(f)?.to_vec()))
            .collect::<Result<Vec<_>, Error>>()?;
        let mut processed = old.concat();
        processed.extend_from_slice(F::bytes(6, head.to_string().as_bytes())?.raw());
        processed.extend_from_slice(&old.concat());
        let actual = wire
            .fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| Ok(wire.encoded_field(f)?.to_vec()))
            .collect::<Result<Vec<_>, Error>>()?
            .concat();
        if actual != processed {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
