// Profile-owned string observations only. No DOM, selection or caret access.
function profileChannelLower(request) {
    if (!request || typeof request.ticket !== "string" ||
        !Number.isSafeInteger(request.stage) || request.stage < 0 ||
        typeof request.query !== "string" || !Array.isArray(request.fields) ||
        request.fields.length > 128 || request.fields.some(s => typeof s !== "string")) return null;
    return {
        ticket: request.ticket, stage: request.stage,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        query: request.query.trim().toLocaleLowerCase(),
        fields: request.fields.map(text => ({
            text, lowered: text.toLowerCase(),
            localeLowered: text.trim().toLocaleLowerCase()
        }))
    };
}

function profileChannelCollation(request) {
    if (!request || typeof request.ticket !== "string" ||
        !Array.isArray(request.names) || request.names.some(s => typeof s !== "string") ||
        new Set(request.names).size !== request.names.length) return null;
    // Same source UI-language default as the existing shared rank renderer.
    const collator = new Intl.Collator("en-US", { numeric: true });
    const compare = (a, b) => a === b ? 0 : a && b ? collator.compare(a, b) : a ? 1 : -1;
    const order = request.names.map((_, i) => i)
        .sort((a, b) => compare(request.names[a], request.names[b]));
    let group = 0;
    const classes = order.map((index, position) => {
        if (position && compare(request.names[order[position - 1]], request.names[index]) !== 0) group++;
        return [index, group];
    });
    let toggle = null;
    try { toggle = localStorage.getItem("TOGGLE_STORAGE-autocomplete_demote_flannel_passthrough_rltv_exact_match"); } catch {}
    return {
        ticket: request.ticket,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        collationLocale: collator.resolvedOptions().locale,
        demotePassthrough: toggle === "on" || toggle === "treatment", classes
    };
}

// JJAw fallback lowering and 5cyv/814n insertion labels are separate operations.
// In particular neither one strips a prefix or trims user-authored whitespace.
function profileGroupStrings(request) {
    if (!request || typeof request.ticket !== "string" || !request.ticket ||
        !Number.isSafeInteger(request.stage) || request.stage < 0 ||
        !Array.isArray(request.rows) || request.rows.length > 128 ||
        request.rows.some(row => !row || typeof row !== "object" || Array.isArray(row) ||
            typeof row.id !== "string" || !row.id || typeof row.name !== "string" ||
            Object.keys(row).some(key => !["id", "name", "handle"].includes(key)) ||
            (Object.hasOwn(row, "handle") && typeof row.handle !== "string" &&
                row.handle !== null && row.handle !== false && row.handle !== 0)) ||
        new Set(request.rows.map(row => row.id)).size !== request.rows.length) return null;
    return {
        ticket: request.ticket, stage: request.stage,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        inputs: request.rows,
        fields: request.rows.map(row => ({
            id: row.id, lowered: row.name.toLocaleLowerCase(),
            label: ("@" + row.handle).normalize("NFC")
        }))
    };
}

// 814n ordinary member insertion normalizes the final display label only.
function listMemberLabels(request) {
    if (!request || typeof request.ticket !== "string" || !Array.isArray(request.labels) || request.labels.length > 128 ||
        !request.labels.every(value => typeof value === "string")) return null;
    return {ticket:request.ticket, labels:request.labels.map(value => value.normalize())};
}
