// Test-only: the shipped private Quill's Delta, without loading DOM/editor code.
const fs=require('node:fs'),vm=require('node:vm'),path=require('node:path'),assert=require('node:assert/strict');
const source=fs.readFileSync(path.join(__dirname,'../../../../../assets/comments-quill/quill-1.3.7.js'),'utf8');
const entry='return __webpack_require__(__webpack_require__.s = 109);';
assert.equal(source.split(entry).length,2);
const context={module:{exports:{}},exports:{},Buffer,process};
vm.runInNewContext(source.replace(entry,'return {default:__webpack_require__(2)};'),context);
const Delta=context.module.exports;
const table=JSON.parse(fs.readFileSync(__dirname+'/emoticons.json','utf8'));
function load(extra={}) {
  const context=vm.createContext(extra);
  for(const name of ['model','smart_quotes','smart_symbols'])vm.runInContext(fs.readFileSync(__dirname+'/'+name+'.js','utf8'),context);
  return {context,smart:context.listCommentDeltaSmartSymbols(Delta,table),model:context.listCommentDeltaModel(Delta)};
}
module.exports={Delta,load,table,clone:value=>JSON.parse(JSON.stringify(value))};
