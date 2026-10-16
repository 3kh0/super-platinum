use serde::Serialize;
#[derive(Default, Serialize)]
pub(super) struct Ledger {
    pub issued: usize,
    pub returns: Vec<(usize, &'static str)>,
    pub outcomes: Vec<(usize, &'static str)>,
    pub transfers: Vec<usize>,
    pub held: bool,
    pub released: bool,
    pub metadata: usize,
    pub bodies: usize,
    active: Option<usize>,
    failed: bool,
}
impl Ledger {
    fn require(&mut self, ok: bool) -> Result<(), String> {
        if self.failed || !ok {
            self.failed = true;
            Err("Cached ledger rejected transition".into())
        } else {
            Ok(())
        }
    }
    pub fn begin(&mut self) -> Result<usize, String> {
        self.require(self.active.is_none() && self.issued < 3)?;
        self.issued += 1;
        self.active = Some(self.issued);
        Ok(self.issued)
    }
    pub fn returned(&mut self, n: usize, kind: &'static str) -> Result<(), String> {
        self.require(
            self.active == Some(n)
                && self.returns.len() + 1 == n
                && kind == if n == 1 { "none" } else { "existing" },
        )?;
        self.returns.push((n, kind));
        if n == 1 {
            self.active = None;
        }
        Ok(())
    }
    pub fn transfer(&mut self, n: usize) -> Result<(), String> {
        self.require(
            n == 2
                && self.active == Some(n)
                && self.returns.last() == Some(&(n, "existing"))
                && self.transfers.is_empty(),
        )?;
        self.transfers.push(n);
        Ok(())
    }
    pub fn saved(&mut self, n: usize, saved: bool) -> Result<(), String> {
        self.require(
            n == 2
                && saved
                && self.active == Some(n)
                && self.transfers == [2]
                && self.outcomes.is_empty(),
        )?;
        self.outcomes.push((n, "saved"));
        self.active = None;
        Ok(())
    }
    pub fn hold(&mut self, n: usize) -> Result<(), String> {
        self.require(
            n == 3
                && self.active == Some(n)
                && self.returns.last() == Some(&(3, "existing"))
                && !self.held
                && self.outcomes == [(2, "saved")],
        )?;
        self.held = true;
        Ok(())
    }
    pub fn metadata(&mut self) -> Result<(), String> {
        self.require(self.held && !self.released && self.metadata == 0 && self.bodies == 0)?;
        self.metadata = 1;
        Ok(())
    }
    pub fn body(&mut self) -> Result<(), String> {
        self.require(self.held && !self.released && self.metadata == 1 && self.bodies == 0)?;
        self.bodies = 1;
        Ok(())
    }
    pub fn release(&mut self) -> Result<(), String> {
        self.require(self.held && !self.released && self.metadata == 1 && self.bodies == 1)?;
        self.released = true;
        Ok(())
    }
    pub fn retired(&mut self, n: usize, canceled: bool) -> Result<(), String> {
        self.require(
            n == 3
                && canceled
                && self.active == Some(3)
                && self.held
                && self.released
                && self.transfers == [2]
                && self.outcomes == [(2, "saved")],
        )?;
        self.outcomes.push((3, "retired-before-transfer"));
        self.active = None;
        Ok(())
    }
    pub fn at(&self, stage: &str) -> bool {
        let (issued, returned, outcomes, held, released, reads) = match stage {
            "initial" => (0, 0, 0, false, false, 0),
            "canceled" => (1, 1, 0, false, false, 0),
            "saved" => (2, 2, 1, false, false, 0),
            "held" => (3, 3, 1, true, false, 0),
            "refreshed" => (3, 3, 1, true, false, 1),
            "retired" => (3, 3, 2, true, true, 1),
            _ => return false,
        };
        !self.failed
            && self.issued == issued
            && self.returns.len() == returned
            && self.outcomes.len() == outcomes
            && self.held == held
            && self.released == released
            && self.metadata == reads
            && self.bodies == reads
            && self.transfers.len() == usize::from(issued >= 2)
            && self.active
                == if matches!(stage, "held" | "refreshed") {
                    Some(3)
                } else {
                    None
                }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn saved() -> Ledger {
        let mut l = Ledger::default();
        l.begin().unwrap();
        l.returned(1, "none").unwrap();
        l.begin().unwrap();
        l.returned(2, "existing").unwrap();
        l.transfer(2).unwrap();
        l.saved(2, true).unwrap();
        l
    }
    #[test]
    fn exact_sequence_has_one_transfer_and_real_retirement() {
        let mut l = saved();
        assert!(l.at("saved"));
        l.begin().unwrap();
        l.returned(3, "existing").unwrap();
        l.hold(3).unwrap();
        assert!(l.at("held"));
        l.metadata().unwrap();
        l.body().unwrap();
        assert!(l.at("refreshed"));
        l.release().unwrap();
        l.retired(3, true).unwrap();
        assert!(l.at("retired"));
        assert_eq!(l.transfers, [2]);
    }
    #[test]
    fn early_or_duplicate_authority_poison_ledger() {
        let mut l = saved();
        assert!(l.release().is_err());
        assert!(l.begin().is_err());
        let mut l = saved();
        l.begin().unwrap();
        l.returned(3, "existing").unwrap();
        l.hold(3).unwrap();
        assert!(l.transfer(3).is_err());
        let mut l = Ledger::default();
        l.begin().unwrap();
        assert!(l.returned(1, "existing").is_err());
        let mut l = saved();
        assert!(l.returned(2, "existing").is_err());
    }
}
