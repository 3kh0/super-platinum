const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const code=fs.readFileSync(__dirname+'/grace_identity.js','utf8');
function setup(){
 const root={},detail={isConnected:true};
 const buttons=Array.from({length:7},(_,i)=>({id:`channel-${i}`,dataset:{channel:`C${i}`},isConnected:true,textContent:'grace',name:{},querySelector(){return this.name}}));
 const nodes={'workflow-management':root,'workflow-activity-detail':detail};
 const context=vm.createContext({window:{},document:{getElementById:id=>nodes[id],querySelectorAll:()=>buttons}});
 const run=()=>vm.runInContext(code,context);
 assert.equal(run().ok,true); return {run,nodes,buttons};
}
let s=setup();s.buttons[0].textContent='grace (archived)';s.buttons[0].svg={newPath:true};assert.equal(s.run().ok,true);
s=setup();s.nodes['workflow-management']={};assert.equal(s.run().ok,false);
s=setup();s.nodes['workflow-activity-detail']={};assert.equal(s.run().ok,false);
s=setup();s.buttons[0].name={};assert.equal(s.run().ok,false);
s=setup();s.buttons[0]={...s.buttons[0]};assert.equal(s.run().ok,false);
s=setup();s.buttons[0].isConnected=false;assert.equal(s.run().ok,false);
s=setup();s.buttons.pop();assert.equal(s.run().ok,false);
console.log('7 actual Activity grace identity JS checks passed');
