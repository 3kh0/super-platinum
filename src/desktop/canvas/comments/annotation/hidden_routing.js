// Original-node hidden/openable routing used by the acknowledged capture bridge.
// `leaves` and `controls` must be captured from that exact original, bounded
// text-only tree; never reconstruct these identities from a receipt or ID.
function canvasAnnotationHiddenRoute({content, range, selection, leaves, controls, start, end, exactEmpty}) {
    if (typeof exactEmpty !== "boolean" || !content?.contains(range.startContainer) ||
        !content.contains(range.endContainer) || !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(end) || start < 0 || end <= start ||
        typeof selection?.containsNode !== "function") throw Error("invalid original annotation range");
    const byNode = new Map(), ids = new Set();
    for (const control of controls) {
        if (!control?.node || !content.contains(control.node) || control.node === content ||
            typeof control.id !== "string" || !control.id ||
            typeof control.hidden !== "boolean" || typeof control.archived !== "boolean" ||
            (control.archived && !control.hidden) || byNode.has(control.node) || ids.has(control.id))
            throw Error("invalid original annotation controls");
        byNode.set(control.node, control); ids.add(control.id);
    }
    // This closed rendering path contains text and transparent span/link nodes,
    // no atoms, focusable controls, trailing BR or navigable-object sections.
    // Retained EditorPosition.annotationBefore/After walk the neighboring
    // character's actual leaf node and then find its containing control. They
    // do not ask Selection.containsNode or compare wrapper outer boundaries.
    const containing = original => {
        let node = original?.nodeType === 1 ? original : original?.parentElement;
        for (; node && node !== content; node = node.parentElement) {
            const control = byNode.get(node);
            if (control) return control;
        }
        return null;
    };
    let offset = 0, text = "";
    const seenLeaves = new Set();
    for (const leaf of leaves) {
        if (leaf.node?.nodeType !== 3 || !content.contains(leaf.node) ||
            seenLeaves.has(leaf.node) || leaf.start !== offset ||
            leaf.end !== offset + leaf.node.data.length || leaf.end > 65536)
            throw Error("invalid original annotation leaves");
        seenLeaves.add(leaf.node); text += leaf.node.data;
        offset = leaf.end;
    }
    const insidePair = at => at > 0 && at < text.length &&
        /[\uD800-\uDBFF]/.test(text[at - 1]) && /[\uDC00-\uDFFF]/.test(text[at]);
    if (end > offset || insidePair(start) || insidePair(end)) throw Error("invalid original annotation offset");
    const before = position => {
        const leaf = leaves.find(value => value.start < position && position <= value.end);
        return leaf ? containing(leaf.node) : null;
    };
    const after = position => {
        const leaf = leaves.find(value => value.start <= position && position < value.end);
        return leaf ? containing(leaf.node) : null;
    };
    const ancestor = range.commonAncestorContainer;
    const inside = containing(ancestor);
    const openInside = inside && !inside.archived ? inside : null;
    let descendant = null;
    // Retained $(range.commonAncestorContainer).find(...) has no descendants
    // for a text common ancestor. Never widen it to the text's element parent.
    if (ancestor.nodeType === 1) for (const control of controls) {
        if (!control.hidden && control.node !== ancestor && ancestor.contains(control.node) &&
            selection.containsNode(control.node, true)) {
            descendant = control; break;
        }
    }
    let selected = null;
    if (!exactEmpty) selected = openInside ?? descendant;
    else {
        for (const value of [openInside, descendant]) {
            if (value && !value.hidden) { selected = value; break; }
        }
        if (!selected) {
            const i = before(start), r = after(start), s = before(end), a = after(end);
            if (i !== r && s !== a && r && !r.archived && r === s) selected = r;
        }
    }
    return selected?.id ?? null;
}
