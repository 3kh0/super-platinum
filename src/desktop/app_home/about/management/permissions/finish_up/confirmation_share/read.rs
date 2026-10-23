//! Query-owned reads preserve raw result ordering for member-info only.
use super::*;
type Ordered = (Value, Option<Vec<String>>);
impl Query {
    pub(super) async fn read(
        &self,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        self.read_guarded(state, request, false, &|_| true)
            .await
            .map(|r| r.0)
    }
    pub(super) async fn read_guarded(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
        member: bool,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Ordered, Error> {
        let observed = state;
        let admitted = || self.current(&observed.peek()) && current(&observed.peek());
        if !admitted() || !self.authorized(state).await || !admitted() {
            return Err(unavailable());
        }
        let cooldown = owned_mut(&mut state.write(), self)
            .and_then(|v| v.retry_after)
            .map(|until| until.saturating_duration_since(tokio::time::Instant::now()))
            .filter(|wait| !wait.is_zero());
        if let Some(wait) = cooldown {
            return Err(Error::RateLimited {
                retry_after_secs: Some(wait.as_secs().saturating_add(1)),
            });
        }
        let result = tokio::select! {
            value=async {
                if crate::fixture::is_fixture() {
                    if member {
                        if let Some(result)=fixture::first_publish::confirmation::share::multiple::member(state,&request).await{return result;}
                        if let Some(result) = fixture::first_publish::confirmation::share::person::member(state,&request).await { return result; }
                        return fixture::first_publish::confirmation::share::dm::member(state,&request).await.unwrap_or_else(||Err(unavailable()));
                    }
                    if let Some(result)=fixture::first_publish::confirmation::share::multiple::read(state,&request).await{return result.map(|v|(v,None))}
                    if let Some(result)=fixture::first_publish::confirmation::share::person::read(state,&request).await {return result.map(|v|(v,None))}
                    if let Some(result)=fixture::first_publish::confirmation::share::read(state,&request).await {return result.map(|v|(v,None))}
                    fixture::first_publish::confirmation::read(state,&request).await.unwrap_or_else(||Err(unavailable())).map(|v|(v,None))
                } else if member {
                    self.ctx().transport.execute_directory_member_info_current(request,admitted).await.map(|v|v.into_parts())
                } else {self.ctx().transport.execute(request).await.map(|v|(v,None))}
            }=>value,
            _=async {while admitted(){tokio::time::sleep(Duration::from_millis(100)).await;}}=>return Err(unavailable()),
        };
        if !admitted() || !native::current(self, None).await || !admitted() {
            return Err(unavailable());
        }
        if let Some(v) = owned_mut(&mut state.write(), self) {
            if let Err(Error::RateLimited {
                retry_after_secs: Some(seconds),
            }) = &result
            {
                v.retry_after =
                    tokio::time::Instant::now().checked_add(Duration::from_secs(*seconds));
            }
            v.reads.push(match &result {
                Ok((raw, _)) => json!({"raw":raw}),
                Err(_) => json!({"failure":"read failed"}),
            });
        }
        result
    }
}
