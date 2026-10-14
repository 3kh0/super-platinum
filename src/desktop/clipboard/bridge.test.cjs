// Synthetic DOM bridge checks only. This never reads the OS clipboard.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '..', 'clipboard.rs'), 'utf8')
    .split('r#"')[1].split('"#')[0];

async function check() {
    let callback, sent, prevented, decoded, finish;
    let scope = 'original';
    const channel = {id: 'channel-composer', tagName: 'TEXTAREA', getAttribute: () => scope};
    const thread = {id: 'thread-composer', tagName: 'TEXTAREA', getAttribute: () => scope};
    vm.runInNewContext(source, {
        document: {
            addEventListener: (name, fn) => { assert.equal(name, 'paste'); callback = fn; },
            getElementById: id => ({'channel-composer': channel, 'thread-composer': thread})[id],
        },
        dioxus: {send: value => { sent = value; }},
        btoa: value => Buffer.from(value, 'binary').toString('base64'),
        Uint8Array,
    });
    function event(target) {
        sent = undefined; prevented = false; decoded = false;
        return {
            target,
            clipboardData: {files: [{name: 'a.png', type: 'image/png', arrayBuffer: () => {
                decoded = true;
                return new Promise(resolve => { finish = resolve; });
            }}]},
            preventDefault: () => { prevented = true; },
        };
    }
    for (const target of [
        {id: 'search', tagName: 'INPUT'},
        {id: 'modal-text', tagName: 'TEXTAREA'},
        {id: 'thread-pane', tagName: 'DIV'},
        {...channel}, // A detached old field with a matching id is not authority.
        null,
    ]) {
        await callback(event(target));
        assert.equal(prevented, false);
        assert.equal(decoded, false);
        assert.equal(sent, undefined);
    }
    scope = '';
    await callback(event(channel));
    assert.equal(prevented, false);
    assert.equal(decoded, false);
    assert.equal(sent, undefined);
    for (const field of [channel, thread]) {
        scope = 'original';
        const pending = callback(event(field));
        assert.equal(prevented, true);
        assert.equal(decoded, true);
        scope = 'replacement';
        finish(new Uint8Array([1, 2, 3]).buffer);
        await pending;
        assert.equal(sent.scope, 'original');
        assert.equal(sent.thread, field === thread);
        assert.equal(sent.files[0].data, 'AQID');
    }
    console.log('clipboard bridge: outside/detached/unscoped fields ignored; channel/thread pre-await ownership preserved');
}
check().catch(error => { console.error(error); process.exitCode = 1; });
