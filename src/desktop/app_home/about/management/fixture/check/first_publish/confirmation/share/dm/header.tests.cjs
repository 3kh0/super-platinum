const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const header=fs.readFileSync(path.join(__dirname,'header.js'),'utf8').trim();
const source=fs.readFileSync(path.join(__dirname,'../../native.js'),'utf8');
const product=fs.readFileSync(path.join(__dirname,'../../../../../../../../../view/chrome.rs'),'utf8');
assert.match(product,/if is_dm\s*\{\s*h2\s*\{\s*button\s*\{\s*class: "header-name"/);
assert.match(product,/else if let Some\(channel\)=channel\s*\{\s*super::channel_details::ChannelDetailsButton/);
function run(dm,{name=dm?'Dana Reviewer':'#review-destination',dmNode=dm,channelNode=!dm,covered=false,timeline=true,composer=true}={}) {
 const h={id:dmNode?'':'channel-details-trigger',textContent:name,isConnected:true,scrollIntoView(){},getBoundingClientRect(){return{left:10,top:10,right:110,bottom:40,width:100,height:30}},contains(e){return e===this}};
 const t={id:'message-timeline',isConnected:true},c={id:'channel-composer',isConnected:true,value:''};
 const document={getElementById(id){return id==='channel-details-trigger'&&channelNode?h:id==='message-timeline'&&timeline?t:id==='channel-composer'&&composer?c:null},querySelector(selector){assert.equal(selector,'.conversation-header h2 > button.header-name');return dmNode?h:null},querySelectorAll(){return[]},elementFromPoint(){return covered?{}:h}};
 let script=source.replace('BASE','({ok:false})');
 if(dm)script=script.replaceAll("document.getElementById('channel-details-trigger')",header).replaceAll("'#review-destination'",JSON.stringify('Dana Reviewer'));
 return vm.runInNewContext(script,{document,window:{__firstPublishRegistry:{ids:new Map(),next:1}},innerWidth:1000,innerHeight:700});
}
assert.equal(run(true).ok,true);
assert.equal(run(true).destination.header.text,'Dana Reviewer');
assert.equal(run(false).ok,true);
assert.equal(run(true,{dmNode:false,channelNode:true}).ok,false);
assert.equal(run(false,{dmNode:true,channelNode:false}).ok,false);
assert.equal(run(true,{name:'Someone else'}).ok,false);
assert.equal(run(true,{covered:true}).ok,false);
assert.equal(run(true,{timeline:false}).ok,false);
assert.equal(run(true,{composer:false}).ok,false);
console.log('9 actual-header branch/exposure/identity checks passed; product selectors source-pinned');
