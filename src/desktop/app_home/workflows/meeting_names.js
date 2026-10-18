// The formatter is the reviewed source-equivalent pure helper. No DOM writes.
function workflowMeetingNames(request, formatter) {
    if (!request || typeof request.generation !== "string" ||
        !request.events || typeof request.events !== "object" || Array.isArray(request.events)) return null;
    const locale = "en-US", timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const nowMs = Date.now(), names = {};
    try {
        for (const [link, input] of Object.entries(request.events)) {
            if (!link || Object.hasOwn(names, link)) return null;
            Object.defineProperty(names, link, {value: formatter.formatEnglish(input, {locale, nowMs, timeZone}), enumerable: true});
        }
    } catch (_) { return null; }
    return {generation: request.generation, locale, time_zone: timeZone, names};
}
