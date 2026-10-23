//! Existing event/document/packet budgets applied before sequence allocation.
use super::*;

// Renderer event budget, including each Enter. This preserves the previous
// maximum of three Enters plus 4096 CONTENT reports; it is not a Slack limit.
pub(in crate::canvas::edit::structure) const MAX_EVENTS: usize = 4099;

pub(in crate::canvas::edit::structure) fn counts(
    appends: usize,
    reports: usize,
    bytes: usize,
) -> Result<(), Error> {
    if appends == 0
        || reports > typing::MAX_REPORTS
        || appends.checked_add(reports).is_none_or(|n| n > MAX_EVENTS)
        || bytes > typing::MAX_REPORT_BYTES
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}

pub(in crate::canvas::edit::structure) fn inventory(
    source_count: usize,
    appends: usize,
    reused: bool,
) -> Result<(), Error> {
    counts(appends, 0, 0)?;
    let created = appends
        .checked_sub(usize::from(reused))
        .ok_or(Error::Unsupported)?;
    if source_count
        .checked_add(created)
        .is_none_or(|n| n > crate::canvas::wire::MAX_SECTIONS)
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}

/// Count exact encoded fields as each row is produced, before retaining another
/// history. Captured inventory shares the checkpoint's existing 8 MiB budget.
pub(super) struct Packet {
    fields: Vec<F>,
    bytes: usize,
}
impl Packet {
    pub(super) fn new(captured_bytes: usize) -> Result<Self, Error> {
        if captured_bytes > crate::canvas::wire::MAX_BYTES {
            return Err(Error::Unsupported);
        }
        Ok(Self {
            fields: Vec::new(),
            bytes: captured_bytes,
        })
    }
    pub(super) fn push(&mut self, field: F) -> Result<(), Error> {
        let bytes = self
            .bytes
            .checked_add(field.raw().len())
            .ok_or(Error::Unsupported)?;
        if bytes > crate::canvas::wire::MAX_BYTES
            || self.fields.len() >= crate::canvas::wire::MAX_FIELDS
        {
            return Err(Error::Unsupported);
        }
        self.bytes = bytes;
        self.fields.push(field);
        Ok(())
    }
    pub(super) fn finish(self) -> Result<RawMessage, Error> {
        Ok(RawMessage::from_fields(&self.fields)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_packet_budget_includes_captured_source_and_encoded_field_overhead() {
        let field = F::bytes(1, &[0; 127]).unwrap();
        assert!(field.raw().len() > 127);
        let mut exact = Packet::new(crate::canvas::wire::MAX_BYTES - field.raw().len()).unwrap();
        exact.push(field.clone()).unwrap();
        assert_eq!(exact.bytes, crate::canvas::wire::MAX_BYTES);
        assert!(exact.push(F::uint(14, 0).unwrap()).is_err());
        // Failure leaves the previously admitted fields intact.
        assert_eq!(exact.finish().unwrap().raw(), field.raw());
        let mut short = Packet::new(crate::canvas::wire::MAX_BYTES - 127).unwrap();
        assert!(short.push(field).is_err());
        assert!(Packet::new(usize::MAX).is_err());
    }
}
