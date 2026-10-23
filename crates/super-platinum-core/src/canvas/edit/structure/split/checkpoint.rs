//! Inert outgoing-wire validation shared with durable recovery. No mutation or
//! prepared capability is constructed from a journal record.
use super::*;
use automerge::{Automerge, ReadDoc};

pub(in crate::canvas::edit::structure) fn validate_checkpoint_split(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    witness: &SplitWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    if original.crdt()?.is_some_and(|wire| wire.has(10)) {
        return Err(Error::Unsupported);
    }
    let verified = SplitWitness::new(
        &witness.before,
        witness.offset,
        &witness.parent,
        &witness.child,
    )?
    .with_child_updates(&witness.updates)?;
    if text::canonical_draft(original)? != verified.before
        || session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
        || child == original.id()
        || !position::modern_id(child)
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    let updates = data.messages(1)?;
    let [parent_update, child_update] = updates.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent_update.message(38)?.ok_or(Error::Unsupported)?;
    let child_wire = child_update.message(38)?.ok_or(Error::Unsupported)?;
    let infos = parent_wire.messages(10)?;
    let [lineage] = infos.as_slice() else {
        return Err(Error::Unsupported);
    };
    let before_bytes = lineage.bytes(7)?.ok_or(Error::Unsupported)?;
    let before = Automerge::load(before_bytes).map_err(|_| Error::Unsupported)?;
    text::crdt::check_document_state(&before, &witness.before, false)?;
    let heads = before.get_heads();
    if heads.len() != 1 {
        return Err(Error::Unsupported);
    }
    let actor = format!("{session}00000000");
    let seed = if let Some(seed) = original.crdt()? {
        let source = text::crdt::validate(&seed, original, &witness.before, None)?;
        if source.save() != before_bytes {
            return Err(Error::Changed);
        }
        seed
    } else {
        let changes = before.get_changes(&[]);
        if changes.len() != 1
            || changes[0].actor_id().to_hex_string() != actor
            || changes[0].seq() != 1
            || !changes[0].deps().is_empty()
        {
            return Err(Error::Unsupported);
        }
        let revision = original.sequence()?.ok_or(Error::Unsupported)?;
        RawMessage::from_fields(&[
            F::bytes(1, original.id().as_bytes())?,
            F::uint(2, revision)?,
            F::uint(3, revision - revision % 1000)?,
            F::bytes(5, before_bytes)?,
            F::bytes(6, heads[0].to_string().as_bytes())?,
            F::bytes(8, heads[0].to_string().as_bytes())?,
        ])?
    };
    let expected_lineage = RawMessage::from_fields(&[
        F::uint(1, 0)?,
        F::bytes(2, actor.as_bytes())?,
        F::bytes(3, child.as_bytes())?,
        F::uint(4, witness.parent.rtml()?.encode_utf16().count() as u64)?,
        F::bytes(
            5,
            serde_json::to_string(&heads.iter().map(ToString::to_string).collect::<Vec<_>>())
                .map_err(|_| Error::Unsupported)?
                .as_bytes(),
        )?,
        F::bytes(
            6,
            format!(
                "{}-{}",
                seed.uint(2)?.ok_or(Error::Unsupported)?,
                seed.string(8)?.ok_or(Error::Unsupported)?
            )
            .as_bytes(),
        )?,
        F::bytes(7, before_bytes)?,
    ])?;
    if *lineage != expected_lineage {
        return Err(Error::Changed);
    }
    let parent_doc =
        text::crdt::validate(&parent_wire.remove(10)?, original, &witness.parent, None)?;
    let additions = parent_doc.get_changes(&heads);
    if additions.len() != 1
        || additions[0].actor_id().to_hex_string() != actor
        || additions[0].deps() != heads.as_slice()
        || before
            .get_changes(&[])
            .iter()
            .any(|c| parent_doc.get_change_by_hash(&c.hash()).is_none())
        || parent_wire.string(12)? != Some(additions[0].hash().to_string().as_str())
    {
        return Err(Error::Unsupported);
    }
    let mut processed = seed
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            Ok(F::bytes(
                6,
                RawMessage::decode(seed.encoded_field(f)?)?
                    .string(6)?
                    .ok_or(Error::Unsupported)?
                    .as_bytes(),
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let old_processed = processed.clone();
    processed.push(F::bytes(6, additions[0].hash().to_string().as_bytes())?);
    let expected_parent_wire = seed
        .remove(4)?
        .remove(7)?
        .replace_fields(
            5,
            &[F::bytes(
                5,
                parent_wire.bytes(5)?.ok_or(Error::Unsupported)?,
            )?],
        )?
        .replace_fields(6, &processed)?
        .replace_fields(
            12,
            &[F::bytes(12, additions[0].hash().to_string().as_bytes())?],
        )?
        .replace_fields(10, &[F::message(10, &expected_lineage)?])?;
    processed.extend(old_processed);
    let current_parent_wire = expected_parent_wire.replace_fields(6, &processed)?;
    // Preserve exact decoding of old durable v3 requests. Fresh preparation and
    // every v8 typed request use the source-observed P + [split] + P formula.
    if parent_wire != current_parent_wire
        && (!witness.updates.is_empty() || parent_wire != expected_parent_wire)
    {
        return Err(Error::Changed);
    }
    super::typing::validate_child(child_update, &child_wire, child, session, witness)?;
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    if *data
        != prepared::project(
            original,
            child,
            position,
            session,
            witness,
            &parent_wire,
            &child_wire,
            &title,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
