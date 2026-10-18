// XbX7 names use ordinary lowercase. hNDQ queries use trim + renderer locale
// lowercase; J3F and whitespace collapse are applied by the retained core port.
// This helper reads no account/global Slack data and does not mutate the DOM.
function workflowDirectoryLower(request) {
    if (!request || typeof request.generation !== "string" ||
        !Number.isSafeInteger(request.stage) || request.stage < 0 ||
        typeof request.query !== "string" || !Array.isArray(request.names) ||
        request.names.length > 128 || request.names.some(s => typeof s !== "string")) return null;
    return {
        generation: request.generation,
        stage: request.stage,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        query: request.query.trim().toLocaleLowerCase(),
        names: request.names.map(s => s.toLowerCase())
    };
}

// MPIM subqueries and displayed conversation names each use a separate hNDQ
// call. Batch only the renderer-owned first step; core applies J3F/collapse.
function workflowDirectoryFields(request) {
    if (!request || typeof request.generation !== "string" ||
        !Number.isSafeInteger(request.stage) || request.stage < 0 ||
        !Array.isArray(request.fields) || request.fields.length > 128 ||
        request.fields.some(s => typeof s !== "string")) return null;
    return {
        generation: request.generation, stage: request.stage,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        fields: request.fields.map(s => s.trim().toLocaleLowerCase())
    };
}
