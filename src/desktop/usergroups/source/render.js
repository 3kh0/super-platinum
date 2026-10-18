// JJAw derives _name_lc using the renderer's default locale, without trim.
// This reads no Slack or DOM data and writes no global state.
function groupSourceLower(request) {
    if (!request || typeof request.generation !== "string" || !request.generation ||
        !Number.isSafeInteger(request.stage) || request.stage < 0 ||
        !Array.isArray(request.fields) || request.fields.length > 128 ||
        request.fields.some(value => typeof value !== "string" || !value)) return null;
    return {
        generation: request.generation, stage: request.stage,
        fields: request.fields.slice(),
        lowered: request.fields.map(value => value.toLocaleLowerCase()),
        locale: Intl.DateTimeFormat().resolvedOptions().locale
    };
}
