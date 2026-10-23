// Save-owned, read-only renderer facts. No popup, DOM, selection or mutation.
function saveChannelOptions(request) {
    if (!request || typeof request.ticket !== 'string') return null;
    const key = 'ml_fuzzy_match_beyond_word_boundaries_v2';
    try {
        // uQr4.nT uses legacy JS unescape, not URI decoding. Keys and '+' are
        // unchanged; repeated keys and repeated scope entries use the last.
        const search = location.search;
        if (typeof search !== 'string') return null;
        let query = search;
        while (query.startsWith('?')) query = query.slice(1);
        const parameters = new Map();
        for (const entry of query.split('&')) {
            const at = entry.indexOf('=');
            const name = at < 0 ? entry : entry.slice(0, at);
            const value = at < 0 ? '' : unescape(entry.slice(at + 1));
            parameters.set(name, value);
        }
        let forced = null;
        const values = parameters.get('force_' + key);
        if (values) for (const item of values.split(',')) {
            const at = item.indexOf(':');
            const scope = at < 0 ? 'all' : item.slice(0, at);
            if (scope === 'all') forced = at < 0 ? item : item.slice(at + 1);
        }
        // Read storage only when the original nullish precedence reaches it.
        let storage = null, storageFailed = false;
        if (forced === null) {
            try { storage = localStorage.getItem('TOGGLE_STORAGE-' + key); }
            catch (_) { storageFailed = true; }
        }
        if (storage !== null && typeof storage !== 'string') return null;
        const group = forced ?? storage ?? 'noGroup';
        return {ticket: request.ticket, key, search, forced, storage, storageFailed, group,
            enabled: group === 'on' || group === 'treatment'};
    } catch (_) { return null; }
}

function saveChannelEligibility(request) {
    if (!request || typeof request.ticket !== 'string' || typeof request.query !== 'string') return null;
    try {
        const segmenter = new Intl.Segmenter(undefined, {granularity: 'grapheme'});
        const iterator = segmenter.segment(request.query)[Symbol.iterator]();
        const segments = [];
        for (let i = 0; i < 4; i++) {
            const next = iterator.next();
            if (next.done) break;
            segments.push(next.value.segment);
        }
        return {ticket: request.ticket, query: request.query,
            locale: Intl.DateTimeFormat().resolvedOptions().locale,
            segmenterLocale: segmenter.resolvedOptions().locale,
            segments, eligible: segments.length === 4};
    } catch (_) { return null; }
}
