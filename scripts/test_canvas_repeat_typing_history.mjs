// Adversarial fixture builder only: valid original-runtime histories with the
// same final text but a different mutation path. Never used as an acceptance gate.
import fs from 'node:fs';
import {A} from '../tmp/canvas-structure-prep/rich-runtime.mjs';
const data=fs.readFileSync(0,'utf8');
if (Buffer.byteLength(data)>1024*1024) throw Error('bounded fixture input');
const {document,mode}=JSON.parse(data);
const source=A.load(new Uint8Array(Buffer.from(document,'base64')));
const changes=A.getAllChanges(source).map(A.decodeChange), first=changes[0];
let doc=A.clone(A.view(source,[first.hash]),{actor:first.actor});
const change=(i,n,s='')=>{doc=A.change(doc,{time:1792000000},d=>A.splice(d,['text'],i,n,s));};
if(mode==='squash')change(0,0,'New paragraph');
else if(mode==='wrong-order'){
  change(0,0,'New');change(3,0,' draftx');change(9,1);
  change(4,0,'parag');change(9,1);change(11,0,'ph');change(13,2);
}else if(mode==='extra-empty-cycle'){
  change(0,0,'New');change(0,3);change(0,0,'x');change(0,1);
}else throw Error('unapproved fixture mode');
process.stdout.write(JSON.stringify({document:Buffer.from(A.save(doc)).toString('base64'),head:A.getHeads(doc)[0],text:doc.text}));
