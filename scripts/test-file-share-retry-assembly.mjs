import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
const candidate=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const rust=fs.readFileSync(path.join(candidate,'src/desktop/file_share/check.rs'),'utf8');
const block=rust.match(/let mut eval = document::eval\(&format!\(([\s\S]*?)\n    \)\);/)[1];
const template=JSON.parse(block.match(/"(?:[^"\\]|\\.)*"/)[0]);
const files=[...block.matchAll(/include_str!\("([^"]+)"\)/g)].map(m=>m[1]);
assert.deepEqual(files,['../view/file_share/check.js','check/retry_editor.js','check/ui.js']);
const parts=files.map(file=>{
 const relative=path.normalize(path.join('src/desktop/file_share',file)),local=path.join(candidate,relative);
 // Installed source test resolves every include locally. Staged candidate tests
 // read unchanged pinned dependency sources from the repository working directory.
 return fs.readFileSync(fs.existsSync(local)?local:path.resolve(relative),'utf8');
});
function assemble(format){let index=0;return format.replace(/\{\}/g,()=>parts[index++]);}
async function execute(source){
 let receives=0;const window={};
 try{await vm.runInNewContext(`(async()=>{${source}\n})()`,{window,dioxus:{recv(){receives++;throw Error('STOP_AFTER_INSTALL');}}});}
 catch(error){return {message:error.message,receives,checker:!!window.__fileShareUI,retry:!!window.__fileShareRetry};}
 throw Error('Fixture unexpectedly ran past bounded receive sentinel');
}
test('actual embedded assembly separates adjacent IIFEs; old composition fails before recv',async()=>{
 const good=await execute(assemble(template));
 assert.deepEqual(good,{message:'STOP_AFTER_INSTALL',receives:1,checker:true,retry:true});
 const bad=await execute(assemble('{}\n{}\n{}'));
 assert.match(bad.message,/is not a function/);
 assert.equal(bad.receives,0);assert.equal(bad.checker,true);assert.equal(bad.retry,false);
});
