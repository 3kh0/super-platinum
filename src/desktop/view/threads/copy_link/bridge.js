// Rust owns the original Copy purpose and physical slot. This bridge owns only
// one captured DOM occurrence, fresh gestures and honest clipboard settlement.
async function runThreadCopyControl(options, environment = globalThis, bridge) {
    const integer = value => Number.isSafeInteger(value) && value >= 0;
    if (!options || !integer(options.id) || !integer(options.mount) ||
        typeof options.nonce !== "string" || !options.nonce ||
        typeof options.scope !== "string" || !options.scope ||
        typeof options.hostId !== "string" || !options.hostId ||
        typeof options.buttonId !== "string" || !options.buttonId ||
        options.hostId === options.buttonId || !bridge ||
        typeof bridge.send !== "function" || typeof bridge.recv !== "function") return;

    const { id, mount, nonce, scope, hostId, buttonId } = options;
    const send = bridge.send, recv = bridge.recv;
    let seq = 0, busy = false, retired = false, enabledSeen = false;
    let observer, button, document, window, navigator, activation, clipboard, writeText;
    let frame, parent, top, nativeThen, resolveLifetime;
    const lifetime = new Promise(resolve => { resolveLifetime = resolve; });
    const emit = (kind, currentSeq, outcome) => {
        const message = { kind, id, mount, nonce, seq: currentSeq };
        if (outcome !== undefined) message.outcome = outcome;
        return send.call(bridge, message);
    };
    const matching = (reply, currentSeq) => reply && typeof reply === "object" &&
        !Array.isArray(reply) && reply.id === id && reply.mount === mount &&
        reply.nonce === nonce && reply.seq === currentSeq;
    const detach = () => {
        if (button) button.removeEventListener("click", onClick, true);
        if (observer) observer.disconnect();
        if (window) window.removeEventListener("pagehide", onRetirement);
        if (document) document.removeEventListener("visibilitychange", onVisibility);
    };
    const finish = () => { retired = true; detach(); resolveLifetime(); };
    const retire = () => {
        if (retired) return;
        retired = true;
        detach();
        // A pending permit or issued Promise must finish its original protocol.
        if (!busy) Promise.resolve().then(() => emit("retired", seq)).catch(() => {}).finally(finish);
    };
    const onRetirement = () => retire();
    const onVisibility = () => { if (document.hidden || document.visibilityState !== "visible") retire(); };
    const visibleAncestors = (node, owner) => {
        if (!node || node.ownerDocument !== owner || node.isConnected !== true ||
            !Array.from(node.getClientRects()).some(rect => rect.width > 0 && rect.height > 0)) return false;
        for (let current = node; current; current = current.parentElement) {
            if (current.ownerDocument !== owner || current.hidden || current.inert ||
                current.hasAttribute("hidden") || current.hasAttribute("inert") ||
                current.getAttribute("aria-hidden") === "true" ||
                current.getAttribute("aria-disabled") === "true" ||
                (current !== button && current.hasAttribute("disabled"))) return false;
            const style = owner.defaultView.getComputedStyle(current);
            if (style.display === "none" || style.visibility === "hidden" ||
                style.visibility === "collapse" || style.contentVisibility === "hidden" ||
                Number.parseFloat(style.opacity) === 0) return false;
        }
        return true;
    };
    const currentNode = (allowDisabled = false) => {
        try {
            if (environment.document !== document || environment.window !== window ||
                window.document !== document || document.defaultView !== window ||
                window.frameElement !== frame || window.parent !== parent || window.top !== top ||
                document.hidden || document.visibilityState !== "visible" ||
                document.getElementById(hostId) !== host || document.getElementById(buttonId) !== button ||
                button.tagName !== "BUTTON" || button.type !== "button" || !host.contains(button) ||
                host.dataset.threadCopyScope !== scope || host.dataset.threadCopyIntent !== String(id) ||
                host.dataset.threadCopyMount !== String(mount) ||
                !visibleAncestors(host, document) || !visibleAncestors(button, document) ||
                (frame && !visibleAncestors(frame, frame.ownerDocument))) return false;
            return allowDisabled || (!button.disabled && !button.matches(":disabled"));
        } catch (_) { return false; }
    };
    const currentCapability = () => {
        try {
            return window.navigator === navigator && environment.navigator === navigator &&
                navigator.userActivation === activation && navigator.clipboard === clipboard &&
                clipboard.writeText === writeText && typeof writeText === "function";
        } catch (_) { return false; }
    };
    const activeGesture = () => {
        try {
            return currentNode() && currentCapability() && activation.isActive === true &&
                document.hasFocus() === true && document.activeElement === button;
        } catch (_) { return false; }
    };
    const observe = () => {
        if (!currentNode(true) || (enabledSeen && !currentNode())) retire();
        else if (currentNode()) enabledSeen = true;
    };
    const settleCall = async url => {
        // The call occurs exactly once. A throw/non-Promise is ambiguous, not a
        // proven rejection or evidence that no clipboard operation was issued.
        let result;
        try { result = writeText.call(clipboard, url); }
        catch (_) { return "unknown"; }
        try {
            // Native Promise brand check, without invoking an arbitrary thenable.
            return await nativeThen.call(result, () => "fulfilled", () => "rejected");
        } catch (_) { return "unknown"; }
    };
    const press = async currentSeq => {
        let terminal = false;
        try {
            await emit("pressed", currentSeq);
            const reply = await recv.call(bridge);
            // Never consume another reply as an alternative permit/replay.
            if (!matching(reply, currentSeq) || !["permit", "deny"].includes(reply.kind)) {
                finish();
                return;
            }
            let outcome = "not_requested";
            if (reply.kind === "permit") {
                if (typeof reply.url !== "string" || reply.url.length === 0) terminal = true;
                else if (!retired && activeGesture()) outcome = await settleCall(reply.url);
            }
            // Issued Promises above are always drained, even after DOM retirement.
            await emit("settled", currentSeq, outcome);
            const ack = await recv.call(bridge);
            if (!matching(ack, currentSeq) || ack.kind !== "ack") {
                finish();
                return;
            }
            busy = false;
            if (terminal || retired || !currentNode() || !currentCapability()) {
                retired = true;
                detach();
                try { await emit("retired", currentSeq); } catch (_) {}
                finish();
            }
        } catch (_) {
            // Pipe ambiguity never becomes an invented settlement or new permit.
            finish();
        }
    };
    function onClick(event) {
        if (event.currentTarget !== button) return;
        event.preventDefault();
        if (retired || busy) return;
        if (!currentNode(true) || !currentCapability()) { retire(); return; }
        if (event.isTrusted !== true || !event.target ||
            event.target.ownerDocument !== document || !button.contains(event.target) || !activeGesture()) return;
        if (seq === Number.MAX_SAFE_INTEGER) { retire(); return; }
        busy = true;
        seq += 1;
        void press(seq);
    }

    let host;
    try {
        document = environment.document;
        window = environment.window;
        navigator = window.navigator;
        activation = navigator.userActivation;
        clipboard = navigator.clipboard;
        writeText = clipboard && clipboard.writeText;
        nativeThen = window.Promise.prototype.then;
        frame = window.frameElement; parent = window.parent; top = window.top;
        host = document.getElementById(hostId);
        button = document.getElementById(buttonId);
        if (!activation || typeof writeText !== "function" || typeof nativeThen !== "function" ||
            typeof window.MutationObserver !== "function") {
            await emit("unsupported", 0);
            return;
        }
        if (!currentNode(true)) {
            await emit("retired", 0);
            return;
        }
        enabledSeen = currentNode();
        observer = new window.MutationObserver(observe);
        observer.observe(document, { attributes: true, childList: true, subtree: true });
        if (frame) observer.observe(frame.ownerDocument, { attributes: true, childList: true, subtree: true });
        button.addEventListener("click", onClick, true);
        window.addEventListener("pagehide", onRetirement);
        document.addEventListener("visibilitychange", onVisibility);
        await emit("mounted", 0);
        await lifetime;
    } catch (_) { finish(); }
}

if (typeof module !== "undefined" && module.exports) module.exports = { runThreadCopyControl };
