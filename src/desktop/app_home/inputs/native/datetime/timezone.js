// Independently implemented from retained detector/label behavior.
// timezone-data.json contains observed timezone identities, aliases and location labels.
// Provenance: app-home-datetime-dispatch-20261025/reference/caption-implementation.
// The English caption template is source-backed; localized prefix catalogs were not executed.
const homeDatetimeZoneIds = new Set(homeDatetimeTimezoneData.zoneIds);
function homeDatetimeOwn(table, key) {
    return Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined;
}
function homeDatetimeDetectZone() {
    let resolved;
    try {
        resolved = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch (_) {
        // The original detector also falls through when Intl is unavailable.
    }
    if (resolved) {
        if (homeDatetimeZoneIds.has(resolved)) return resolved;
        for (const table of [homeDatetimeTimezoneData.manual,
            homeDatetimeTimezoneData.equivalent, homeDatetimeTimezoneData.fallback]) {
            const mapped = homeDatetimeOwn(table, resolved);
            if (mapped) return mapped;
        }
    }
    const offsets = homeDatetimeTimezoneData.probeDatesUtc.map(
        instant => -new Date(Date.parse(instant)).getTimezoneOffset());
    for (let count = 1; count <= offsets.length; count++) {
        const mapped = homeDatetimeOwn(homeDatetimeTimezoneData.offsetProbe,
            offsets.slice(0, count).join(":"));
        if (mapped) return mapped;
    }
    return "America/Los_Angeles";
}
function homeDatetimeCanonicalZone(zone) {
    if (homeDatetimeZoneIds.has(zone)) return zone;
    // Label canonicalization has a different alias order from detection.
    return homeDatetimeOwn(homeDatetimeTimezoneData.equivalent, zone)
        || homeDatetimeOwn(homeDatetimeTimezoneData.manual, zone) || zone;
}
function homeDatetimeLocation(zone, locale) {
    const index = homeDatetimeTimezoneData.zoneIds.indexOf(homeDatetimeCanonicalZone(zone));
    if (index < 0) return undefined;
    const labels = homeDatetimeOwn(homeDatetimeTimezoneData.locations, locale)
        || homeDatetimeTimezoneData.locations["en-US"];
    return labels[index];
}
function homeDatetimeCaption(locale) {
    const label = homeDatetimeLocation(homeDatetimeDetectZone(), locale);
    return label ? `Time zone: ${label}` : "";
}
