// Observe one original rendered ImageContent. No pixels or URLs cross the bridge.
// Rust retains the exact canonical section before acknowledging this node.
async function captureCanvasAnnotationImage(options, channel) {
    const roots = [...document.querySelectorAll('.canvas-dialog')].filter(n =>
        n.isConnected && n.dataset.dialogGeneration === options.visit);
    if (roots.length !== 1) throw Error('image owner unavailable');
    const root = roots[0];
    const rows = [...root.querySelectorAll('[data-canvas-image-section]')].filter(n =>
        n.dataset.canvasImageSection === options.section);
    if (rows.length !== 1) throw Error('image section unavailable');
    const row = rows[0], body = row.querySelector('[data-canvas-image-body]');
    const identity = () => row.dataset.canvasImageVisit === options.visit &&
        row.dataset.canvasImageRead === options.read &&
        row.dataset.canvasImageSection === options.section &&
        row.dataset.canvasImageMedia === options.media;
    if (!body || !identity() || row.dataset.canvasImageAvailability === 'pending')
        throw Error('image not loaded');
    const available = row.dataset.canvasImageAvailability;
    const images = [...body.querySelectorAll('img')];
    if (!['ready', 'unavailable'].includes(available) || images.length > 1 ||
        (available === 'ready') !== (images.length === 1)) throw Error('image branch changed');
    const image = images[0] ?? null;
    const signature = image && [image.getAttribute('src'), image.currentSrc, image.complete,
        image.width, image.height, image.naturalWidth, image.naturalHeight];
    const visible = node => {
        const style = getComputedStyle(node), rect = node.getBoundingClientRect();
        return !node.hidden && node.getAttribute('aria-hidden') !== 'true' &&
            style.display !== 'none' && style.visibility === 'visible' &&
            rect.width > 0 && rect.height > 0;
    };
    if (!visible(root) || !visible(row) || !visible(body) ||
        (image && (!visible(image) || !image.complete || image.getAttribute('src') !== options.media)))
        throw Error('image not visible or loaded');
    const tree = [...body.childNodes];
    let mutation = false;
    const observer = new MutationObserver(() => { mutation = true; });
    observer.observe(row, {subtree:true, attributes:true, childList:true, characterData:true});
    const current = () => {
        const currentRoots = [...document.querySelectorAll('.canvas-dialog')].filter(n =>
            n.isConnected && n.dataset.dialogGeneration === options.visit);
        if (mutation || observer.takeRecords().length || currentRoots.length !== 1 ||
            currentRoots[0] !== root || !root.isConnected || !row.isConnected ||
            !visible(root) || !visible(row) || !visible(body) ||
            !root.contains(row) || !row.contains(body) || !identity() ||
            row.dataset.canvasImageAvailability !== available ||
            [...body.childNodes].some((n,i) => n !== tree[i]) || body.childNodes.length !== tree.length ||
            [...root.querySelectorAll('[data-canvas-image-section]')].filter(n => n.dataset.canvasImageSection === options.section).length !== 1)
            return false;
        if (!image) return body.querySelectorAll('img').length === 0;
        const now = [image.getAttribute('src'), image.currentSrc, image.complete,
            image.width, image.height, image.naturalWidth, image.naturalHeight];
        return visible(image) && body.querySelectorAll('img').length === 1 && body.querySelector('img') === image &&
            now.every((n,i) => n === signature[i]);
    };
    let acknowledgmentTimer = null;
    try {
        let outcome = 'no_image';
        if (image) {
            // Match ImageContent's actual canvas allocation/export outcome.
            // Browser limits are observed, not replaced by a local pixel cap.
            const width = Math.max(image.width, image.naturalWidth);
            const height = Math.max(image.height, image.naturalHeight);
            if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
                width < 0 || height < 0)
                throw Error('image export limit');
            const canvas = document.createElement('canvas');
            canvas.width = width; canvas.height = height;
            const context = canvas.getContext('2d');
            if (!(context instanceof CanvasRenderingContext2D)) throw Error('image export unavailable');
            try {
                context.drawImage(image, 0, 0, width, height);
                outcome = canvas.toDataURL().length ? 'exported' : 'empty_data';
            } catch (_) {
                outcome = 'export_error';
            } finally {
                canvas.width = 0; canvas.height = 0;
            }
        }
        if (!current()) throw Error('image changed during export');
        const result = {token:options.token, section:options.section, visit:options.visit,
            read:options.read, media:options.media, outcome};
        channel.send(result);
        // Host rejection, timeout or Eval abandonment does not cancel recv().
        // Bound this lifetime independently so the original DOM observer is
        // released even when Rust never sends an acknowledgment.
        const acknowledgment = await new Promise((resolve, reject) => {
            acknowledgmentTimer = setTimeout(() => reject(Error('image acknowledgment timed out')), 2000);
            Promise.resolve().then(() => channel.recv()).then(resolve, reject);
        });
        if (acknowledgment?.token !== options.token || acknowledgment?.accepted !== true || !current())
            throw Error('image custody changed');
        return result;
    } finally {
        if (acknowledgmentTimer !== null) clearTimeout(acknowledgmentTimer);
        observer.disconnect();
    }
}
