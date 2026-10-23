// Offline semantic decoder. Only serialized fixture documents enter this process.
import fs from 'node:fs';
import crypto from 'node:crypto';
import {A} from '../tmp/canvas-structure-prep/rich-runtime.mjs';

const input = fs.readFileSync(0, 'utf8');
if (Buffer.byteLength(input) > 3 * 1024 * 1024) throw Error('History decoder input exceeds bound');
const values = JSON.parse(input);
if (!Array.isArray(values) || values.length < 1 || values.length > 2) throw Error('Expected one or two fixture histories');
const result = values.map(encoded => {
  if (typeof encoded !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) throw Error('Invalid encoded history');
  const bytes = Buffer.from(encoded, 'base64');
  if (!bytes.length || bytes.length > 1024 * 1024) throw Error('History byte bound');
  const doc = A.load(new Uint8Array(bytes));
  const changes = A.getAllChanges(doc);
  if (changes.length > 4096) throw Error('History change bound');
  return {
    sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
    heads: A.getHeads(doc),
    changes: changes.map(bytes => {
      const decoded = A.decodeChange(bytes), view = A.view(doc, [decoded.hash]);
      return {hash: decoded.hash, actor: decoded.actor, seq: decoded.seq, deps: decoded.deps,
        raw: Buffer.from(bytes).toString('base64'), text: view.text, deleted: view.deleted,
        keys: Object.keys(view).sort(), marks: A.marks(view, ['text']), ops: decoded.ops,
        patches: A.diff(doc, decoded.deps, [decoded.hash])};
    }),
  };
});
process.stdout.write(JSON.stringify(result));
