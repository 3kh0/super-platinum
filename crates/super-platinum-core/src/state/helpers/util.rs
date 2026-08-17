pub(super) fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|value| !value.is_empty())
}
