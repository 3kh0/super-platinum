//! One ordinary-member category bound to the original neutral caret lease.
use super::*;
use crate::profile_edit::sections::rich::search::channel_host::Host;
pub(crate) async fn run_members(
    mut state: Signal<ShellState>,
    lease: Lease,
    raw_query: String,
    on_results: impl Fn(Presentation) + 'static,
) -> Result<(), Error> {
    {
        let mut shell = state.write();
        if !lease.can_dispatch(&shell) || !raw_query.starts_with(['@', '\u{ff20}']) {
            return Err(unavailable());
        }
        let q = shell
            .slack_lists
            .comments
            .completion
            .query
            .as_mut()
            .ok_or_else(unavailable)?;
        if !start(q, Kind::Member) {
            return Err(unavailable());
        }
    }
    let publish_lease = lease.clone();
    let mut operation_state = state;
    let operation = async {
        let context = Arc::new(
            member_context::prepare(operation_state, &lease.binding, |shell| {
                lease.can_dispatch(shell)
            })
            .await?,
        );
        {
            let mut shell = operation_state.write();
            if !lease.current(&shell) || !context.current(&shell) {
                return Err(unavailable());
            }
            shell
                .slack_lists
                .comments
                .completion
                .query
                .as_mut()
                .unwrap()
                .member_context = Some(context);
        }
        search::run_list_members(
            operation_state,
            lease.clone(),
            raw_query,
            move |mut state, batch| {
                let presentation =
                    publish(&mut state.write(), &publish_lease, Batch::Member(batch))
                        .ok_or_else(unavailable)?;
                on_results(presentation);
                Ok(())
            },
        )
        .await
    };
    let result = tokio::select! {biased;
        _=retired(state,&lease)=>{close(&mut state.write(),&lease);Err(unavailable())},
        result=operation=>result,
    };
    if let Err(error) = &result {
        let mut shell = state.write();
        if matches!(error, Error::Api(code) if code == "list_completion_source_retired") {
            close(&mut shell, &lease);
        } else if lease.current(&shell) {
            lease.retain_failure(&mut shell, error);
            let q = shell
                .slack_lists
                .comments
                .completion
                .query
                .as_mut()
                .unwrap();
            q.batch = None;
            q.values.clear();
        } else {
            close(&mut shell, &lease);
        }
    }
    result
}
