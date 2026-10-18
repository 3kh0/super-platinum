// Read-only renderer observations. Equality classes retain actual numeric
// collation ties without allocating an all-pairs comparison table.
function workflowRankingCollation(request) {
    if (!request || typeof request.generation !== "string" ||
        !Array.isArray(request.names) ||
        request.names.some(name => typeof name !== "string") ||
        new Set(request.names).size !== request.names.length) return null;
    // fFQY defaults to en-US; GQhO replaces it only on a supported app UI
    // language change. This native UI currently has no alternate UI catalog.
    // Query toLocaleLowerCase remains a separate host-locale observation.
    const collator = new Intl.Collator("en-US", { numeric: true });
    const compare = (a, b) => a === b ? 0 : a && b ? collator.compare(a, b) : a ? 1 : -1;
    const order = request.names.map((_, index) => index)
        .sort((a, b) => compare(request.names[a], request.names[b]));
    let group = 0;
    const classes = order.map((index, position) => {
        if (position && compare(request.names[order[position - 1]], request.names[index]) !== 0) group++;
        return [index, group];
    });
    // Actual p8lb/oTUI session-toggle storage observation. This native app does
    // not accept Slack URL debug-toggle overrides. Storage failure has the
    // source's null/noGroup semantics, not an invented API assignment.
    let toggle = null;
    try { toggle = localStorage.getItem("TOGGLE_STORAGE-autocomplete_demote_flannel_passthrough_rltv_exact_match"); } catch {}
    return {
        generation: request.generation,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        collationLocale: collator.resolvedOptions().locale,
        demotePassthrough: toggle === "on" || toggle === "treatment",
        classes
    };
}
