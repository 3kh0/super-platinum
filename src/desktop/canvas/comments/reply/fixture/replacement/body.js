// Passive fixture observation of the unchanged native finish handshake.
function canvasReturnBodyBridge(options, bridge, environment=globalThis) {
 const clone=v=>JSON.parse(JSON.stringify(v));
 const owner={scope:options.scope,mount:options.mount,generation:options.generation};
 const registry=environment.__canvasReturnBodyCaptures??=new Map();
 if(!(registry instanceof Map)||registry.has(owner.mount)||registry.size>=8)throw Error('Return body registry exhausted');
 const record={...owner,commands:[],captures:[],acks:[],closed:false};
 registry.set(owner.mount,{state:()=>clone(record)});
 const matches=v=>v?.scope===owner.scope&&v?.mount===owner.mount;
 return {
  async send(value){
   // Record before forwarding so an immediate genuine ACK can be correlated.
   if(value?.kind==='capture'&&value.finalCapture===true){
    if(!matches(value)||value.generation!==owner.generation||record.captures.length>=2)throw Error('Original final capture changed');
    record.captures.push(clone(value));
   }
   return await bridge.send(value);
  },
  async recv(){const value=await bridge.recv();
   if(value?.kind==='finish'){
    if(!matches(value)||record.commands.length>=2)throw Error('Original finish owner changed');
    record.commands.push(clone(value));
   }else if(value?.kind==='ack'&&record.captures.some(c=>c.sequence===value.sequence)){
    if(!matches(value)||record.acks.length>=2)throw Error('Original final ACK changed');record.acks.push(clone(value));
   }
   return value;
  },
  close(){record.closed=true;}
 };
}
