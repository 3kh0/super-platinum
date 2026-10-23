// Save-owned string observations only. No popup, DOM or mutable cache.
function saveMemberFacts(request) {
    if (!request || typeof request.ticket !== 'string' ||
        typeof request.query !== 'string' || !Array.isArray(request.names) ||
        request.names.length > 128 || request.names.some(s => typeof s !== 'string')) return null;
    // KhGM checks English me/you and the utility translations. This caller
    // supplies the application's existing English presentation explicitly;
    // it does not claim to have read a Slack account translation catalogue.
    return {
        ticket: request.ticket, query: request.query,
        locale: Intl.DateTimeFormat().resolvedOptions().locale,
        presentation: {locale: 'en-US', origin: 'application-English', me: 'me', you: 'you'},
        maybeSelf: request.query === 'me' || request.query === 'you',
        fields: request.names.map(text => ({text, lowered: text.toLocaleLowerCase()}))
    };
}
