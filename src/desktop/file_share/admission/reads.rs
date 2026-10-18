//! Cooldowns belong to credentials, not the replaceable picker allocation.
use super::*;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::transport::FileShareReadGate;
static GATES: OnceLock<Mutex<BTreeMap<Key, Arc<FileShareReadGate>>>> = OnceLock::new();
fn acquire(
    map: &mut BTreeMap<Key, Arc<FileShareReadGate>>,
    key: &Key,
) -> Result<Arc<FileShareReadGate>, Error> {
    if let Some(gate) = map.get(key) {
        return Ok(gate.clone());
    }
    // Neither a pending request nor an unexpired deadline can be evicted by a
    // reopen, query change, credential refresh, or pressure from another owner.
    map.retain(|_, gate| Arc::strong_count(gate) > 1 || !gate.expired());
    if map.len() >= 64 {
        return Err(Error::Api("file_share_read_owner_capacity".into()));
    }
    let gate = Arc::new(FileShareReadGate::default());
    map.insert(key.clone(), gate.clone());
    Ok(gate)
}
impl Context {
    pub(super) async fn live_read(
        &self,
        transport: &Transport,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        let gate = {
            let mut map = GATES
                .get_or_init(Default::default)
                .lock()
                .map_err(|_| Error::Api("file_share_read_owner_lost".into()))?;
            acquire(&mut map, &self.key)?
        };
        transport.execute_file_share_read(request, &gate).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_capture_and_replaced_credentials_keep_original_gate() {
        let mut map = BTreeMap::new();
        let key = Key {
            account: "fixture-account".into(),
            team: "T1".into(),
            user: "U1".into(),
        };
        let first = acquire(&mut map, &key).unwrap();
        for _ in 0..5 {
            assert!(Arc::ptr_eq(&first, &acquire(&mut map, &key).unwrap()));
        }
        let other = Key {
            team: "T2".into(),
            ..key.clone()
        };
        assert!(!Arc::ptr_eq(&first, &acquire(&mut map, &other).unwrap()));
    }
}
