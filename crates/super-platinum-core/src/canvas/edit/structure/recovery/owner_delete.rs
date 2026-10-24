//! V1 already stores the whole source and outgoing request. Derive a bounded
//! pair proof from those bytes; decoding never creates an executable intent.
use super::*;
use automerge::ReadDoc;

impl Checkpoint {
    pub fn split_owner_child(&self) -> Result<Option<Section>, Error> {
        self.validate()?;
        self.owner_delete_child()
    }

    fn owner_delete_child(&self) -> Result<Option<Section>, Error> {
        if !self.is_delete() {
            return Ok(None);
        }
        let rows = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        let owner = rows
            .iter()
            .find(|r| r.id() == self.target)
            .ok_or(Error::Unsupported)?;
        if !owner.crdt()?.is_some_and(|w| w.has(10)) {
            return Ok(None);
        }
        Ok(Some(super::super::owner_delete::live_pair(
            &rows, owner, 0,
        )?))
    }

    pub(super) fn validate_owner_delete(&self) -> Result<(), Error> {
        if self.owner_delete_child()?.is_none() {
            return Ok(());
        }
        let rows = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        let owner = rows
            .iter()
            .find(|r| r.id() == self.target)
            .ok_or(Error::Unsupported)?;
        let draft = TextDraft::from_rtml(&self.draft)?;
        if text::canonical_draft(owner)? != draft {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        let update = data.message(1)?.ok_or(Error::Unsupported)?;
        let wire = update.message(38)?.ok_or(Error::Unsupported)?;
        let source = owner.crdt()?.ok_or(Error::Unsupported)?;
        super::super::owner_delete::transition(owner, &wire, owner, &draft)?;
        super::super::owner_delete::history(
            &wire,
            owner,
            &draft,
            true,
            None,
            position::modern_id(owner.id()),
        )?;
        let expected = RawMessage::from_fields(&[
            F::bytes(1, self.target.as_bytes())?,
            F::uint(2, owner.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(3, 1)?,
            F::uint(9, 0)?,
            F::message(13, &RawMessage::decode(&[])?)?,
            F::uint(19, 1)?,
            F::uint(20, 1)?,
            F::bytes(33, format!("{}-0", self.session).as_bytes())?,
            F::message(38, &wire)?,
        ])?;
        if update != expected {
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
        // Retain source header/lineage/extension order exactly. Only the
        // observed outgoing lease/dirty/history/processed/current fields vary.
        let mut a = source.clone();
        let mut b = wire.clone();
        for tag in [4, 5, 6, 7, 12] {
            a = a.remove(tag)?;
            b = b.remove(tag)?;
        }
        if a != b || wire.has(4) || wire.has(7) {
            return Err(Error::Unsupported);
        }
        let old = source
            .fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| Ok(source.encoded_field(f)?.to_vec()))
            .collect::<Result<Vec<_>, Error>>()?;
        let mut expected_heads = old.concat();
        expected_heads.extend_from_slice(F::bytes(6, head.to_string().as_bytes())?.raw());
        expected_heads.extend_from_slice(&old.concat());
        let actual_heads = wire
            .fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| Ok(wire.encoded_field(f)?.to_vec()))
            .collect::<Result<Vec<_>, Error>>()?
            .concat();
        if actual_heads != expected_heads {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
