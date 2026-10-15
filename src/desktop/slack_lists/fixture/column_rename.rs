use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) -> Value {
    super::hydration::install(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Field rename review");
    f.document["created"] = json!(1700000000);
    f.document["user"] = json!("U1");
    f.document["list_metadata"]["schema"][0]["options"] = json!({"format":"text"});
    f.document["list_metadata"]["schema"][0]["opaque"] = json!({"keep":1});
    f.document["list_metadata"]["opaque"] = json!({"keep":2});
    let server = f.document.clone();
    super::refresh(shell);

    server
}
