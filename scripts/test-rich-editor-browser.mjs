// Runs the existing actual-DOM suite in an isolated installed Chromium browser.
// No app process, Slack session, packages, or external page is involved.
import {createServer} from 'node:http';
import {execFile} from 'node:child_process';
import {readFile, mkdtemp, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const browser=process.env.RICH_EDITOR_TEST_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const profile=await mkdtemp(path.join(tmpdir(),'super-platinum-editor-browser-'));
// Exact local Material helper data and product sizing for the empty-Canvas
// regression, without copying an icon path or fetching any external resource.
const icons=await readFile(path.join(root,'src/desktop/icons.rs'),'utf8');
const canvasPath=icons.match(/Self::Description\s*=>\s*\{\s*"([^"]+)"/)?.[1];
const canvasCss=(await readFile(path.join(root,'src/desktop/styles/later.css'),'utf8'))
  .split(/\r?\n/).find(line=>line.startsWith('.rich-editor-empty-canvas >'));
if(!canvasPath||!canvasCss)throw new Error('Missing source-owned empty Canvas icon/sizing.');
const canvasIcon=`<svg class="material-icon rich-editor-canvas-symbol" viewBox="0 -960 960 960" aria-hidden="true"><path fill="currentColor" d="${canvasPath}"></path></svg>`;
const files=new Map(await Promise.all([
  ['/editor.js',process.env.RICH_EDITOR_TEST_SOURCE || 'src/desktop/rich_editor.js'],
  ['/tests.js',process.env.RICH_EDITOR_TEST_SUITE || 'scripts/rich-editor.browser-tests.js'],
].map(async ([url,file])=>[url,await readFile(path.resolve(root,file))])));
let result=null;
const server=createServer((req,res)=>{
  if(req.url==='/result'&&req.method==='POST'){
    let body='';req.on('data',chunk=>{body+=chunk;if(body.length>1048576)req.destroy();});
    req.on('end',()=>{try{result=JSON.parse(body);res.end('ok');}catch{res.writeHead(400).end();}});return;
  }
  if(req.url==='/'){
    res.setHeader('Content-Type','text/html; charset=utf-8');
    res.end(`<!doctype html><meta charset="utf-8"><script src="/editor.js"></script><script src="/tests.js"></script><body><script>
      (async()=>{let result;try{window.richEditorTestFocus=${JSON.stringify(process.env.RICH_EDITOR_TEST_FOCUS || '')};window.richEditorTestCanvasIcon=${JSON.stringify(canvasIcon)};window.richEditorTestCanvasCss=${JSON.stringify(canvasCss)};result={ok:true,tests:runRichEditorTests()};}catch(error){result={ok:false,error:String(error),stack:error.stack,completed:window.richEditorTestResults||[]};}
      await fetch('/result',{method:'POST',body:JSON.stringify(result)});})();
    </script>`);return;
  }
  if(files.has(req.url)){res.setHeader('Content-Type','application/javascript; charset=utf-8');res.end(files.get(req.url));return;}
  res.writeHead(404).end();
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
  await new Promise((resolve,reject)=>execFile(browser,[
    '--headless=new','--no-first-run','--no-default-browser-check','--disable-sync',
    '--disable-background-networking',`--user-data-dir=${profile}`,
    '--dump-dom','--virtual-time-budget=10000',`http://127.0.0.1:${server.address().port}/`,
  ],{timeout:60000,maxBuffer:2097152,windowsHide:true},(error,_out,err)=>error?reject(new Error(`${error.message}\n${err.slice(-2000)}`)):resolve()));
  if(!result)throw new Error('Browser did not return the actual editor test receipt.');
  const receipt={...result,browser,profile,environment:'isolated headless Chromium; actual DOM selection/execCommand; no native app or layout acceptance'};
  if(process.argv[2])await writeFile(process.argv[2],JSON.stringify(receipt,null,2)+'\n');
  console.log(JSON.stringify(receipt,null,2));
  if(!result.ok)process.exitCode=1;
}finally{await new Promise(resolve=>server.close(resolve));}
