//! One original result/observer for a combined fixture, independent family fences.
use super::*;
use directory::{
    fan_in::{InitialGate, Provider},
    read_scope::ReadScope,
};
use exchange::Exchange;
use tokio::sync::Mutex;

struct Settlement {
    exchange: Option<Exchange>,
    result: Option<Result<Value, Error>>,
}
pub(in crate::app_home::inputs::channels) struct Bundle {
    scope: ReadScope,
    settlement: Mutex<Settlement>,
}
impl Bundle {
    pub(in crate::app_home::inputs::channels) fn new(exchange: Exchange, scope: ReadScope) -> Self {
        Self {
            scope,
            settlement: Mutex::new(Settlement {
                exchange: Some(exchange),
                result: None,
            }),
        }
    }
    pub(in crate::app_home::inputs::channels) async fn family(
        &self,
        provider: Provider,
        gate: InitialGate,
    ) -> Result<Option<Value>, Error> {
        if !gate.wait().await {
            return Ok(None);
        }
        let result = {
            let mut settlement = self.settlement.lock().await;
            if settlement.result.is_none() {
                let result = settlement
                    .exchange
                    .as_mut()
                    .expect("initial exchange")
                    .receive()
                    .await;
                self.scope.observe_attempt(&result);
                settlement.result = Some(result);
                // Release the Harness lane before either family can need a successor.
                drop(settlement.exchange.take());
            }
            settlement.result.as_ref().unwrap().clone()
        };
        // Cached initial consumers never repeat observation or retirement conversion.
        if !self.scope.current() {
            return Ok(None);
        }
        let value = result?;
        value
            .get(provider.key())
            .cloned()
            .map(Some)
            .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
    }
}

#[cfg(test)]
mod tests;
