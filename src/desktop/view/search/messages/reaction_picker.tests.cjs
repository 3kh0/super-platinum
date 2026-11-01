// Pure node-custody/focus contract; not WebView input, layout or native proof.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const mount=vm.runInNewContext(fs.readFileSync(path.join(__dirname,'reaction_picker.js'),'utf8')+';mountSearchReactionPicker');
function rig(){
 const body={},nodes=new Map();let observer;
 const document={body,activeElement:body,getElementById:id=>nodes.get(id)||null};
 function node(){return{isConnected:true,disabled:false,calls:0,closest:()=>null,getClientRects:()=>[{}],focus(){this.calls++;document.activeElement=this}}}
 const opener=node(),input=node(),parent=node(),host=node(),dialog=node();
 parent.dataset={searchReactionFrame:'original'};host.dataset={searchReactionPicker:'7'};
 parent.contains=n=>n===opener;host.contains=n=>n===dialog||n===input;dialog.querySelector=()=>input;
 nodes.set('shell-overlay-dialog',parent);nodes.set('search-reaction-picker',host);nodes.set('search-reaction-dialog',dialog);nodes.set('opener',opener);
 class MutationObserver{constructor(callback){observer=this;this.fire=callback}observe(){}disconnect(){this.closed=true}}
 return{document,nodes,parent,host,dialog,opener,input,environment:{document,MutationObserver},fire:()=>observer.fire()};
}
test('original dialog focus, exact-parent restoration and changed/foreign-owner refusal',()=>{
 let r=rig();assert.equal(mount({id:7,opener:'opener'},r.environment),true);assert.equal(r.input.calls,1);
 r.document.activeElement=r.document.body;r.host.isConnected=false;r.nodes.delete('search-reaction-picker');r.fire();assert.equal(r.opener.calls,1);
 r=rig();mount({id:7,opener:'opener'},r.environment);r.document.activeElement=r.document.body;r.parent.dataset.searchReactionFrame='successor';r.host.isConnected=false;r.nodes.delete('search-reaction-picker');r.fire();assert.equal(r.opener.calls,0);
 r=rig();r.host.dataset.searchReactionPicker='8';assert.equal(mount({id:7,opener:'opener'},r.environment),false);assert.equal(r.input.calls,0);
 r=rig();r.document.activeElement={isConnected:true};mount({id:7,opener:'opener'},r.environment);assert.equal(r.input.calls,0);
 r=rig();mount({id:7,opener:'opener'},r.environment);r.document.activeElement=r.document.body;r.parent.dataset.searchReactionFrame='changed';r.fire();assert.equal(r.input.calls,1);
});
