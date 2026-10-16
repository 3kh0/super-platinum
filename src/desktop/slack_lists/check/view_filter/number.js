(()=>{
 const api=window.__listViewFilter,q=s=>document.querySelector(s);let typed=null,epoch=null,witness=null;
 const native=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'validity');
 const value=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');
 const input=()=>api.rowControl('C_NUMBER','value');
 const epochOf=e=>e?.closest('[data-filter-row]')?.dataset.filterEpoch;
 const owned=e=>e instanceof HTMLInputElement&&e.isConnected&&input()===e&&e.type==='number'&&e.step==='any'&&e.closest('[data-filter-row]')?.querySelector('[data-filter-field]')?.value==='C_NUMBER'&&!!epochOf(e);
 const operator=()=>api.rowControl('C_NUMBER','operator')?.value;
 api.numberState=()=>{const e=input(),v=e&&native.get.call(e);return{ok:owned(e),value:e?.value,valid:v?.valid,badInput:v?.badInput,disabled:e?.disabled,choice:epochOf(e),operator:operator(),sameTyped:e===typed,rotated:e!==typed&&epochOf(e)!==epoch};};
 api.typeNumber=()=>{const e=input();const reveal=api.reveal(e,'number-input');if(!owned(e)||e.disabled||!reveal.ok||!api.hit(e))return{ok:false};typed=e;epoch=epochOf(e);e.focus();for(const v of ['1','12.50']){value.set.call(e,v);e.dispatchEvent(new Event('input',{bubbles:true}));}return{ok:true,values:['1','12.50'],choice:epoch,reveal};};
 api.sameNumber=()=>{const v=api.numberState();return{...v,ok:v.ok&&v.sameTyped&&v.choice===epoch&&v.value==='12.50'&&v.valid&&!v.badInput};};
 api.rotatedNumber=()=>{const v=api.numberState();return{...v,ok:v.ok&&v.rotated&&v.value==='12.50'&&v.operator==='less_than'&&v.valid&&!v.badInput};};
 api.armValidity=invalid=>{
  const e=input();if(witness||!owned(e)||e.disabled||e.value!=='12.50')return{ok:false};
  const old=Object.getOwnPropertyDescriptor(e,'validity'),before=native.get.call(e),message=before.customError?e.validationMessage:'';
  if(invalid)e.setCustomValidity('Owned Number filter validity test');
  const start=native.get.call(e);const records=[];
  // Observe the real production snapshot's native getter without changing its result.
  Object.defineProperty(e,'validity',{configurable:true,get(){const v=native.get.call(e);records.push({value:value.get.call(e),valid:v.valid,badInput:v.badInput});return v;}});
  witness={e,old,message,records,choice:epochOf(e),invalid};
  return{ok:true,value:e.value,valid:start.valid,badInput:start.badInput,choice:witness.choice,syntheticCustomValidity:invalid};
 };
 api.validityRead=()=>({ok:!!witness&&witness.records.length>0,reads:witness?.records.slice()||[]});
 api.restoreValidity=()=>{const w=witness;if(!w)return{ok:false};witness=null;
  // Restore exact node properties even when a later ownership check fails.
  if(w.old)Object.defineProperty(w.e,'validity',w.old);else delete w.e.validity;
  if(w.invalid)w.e.setCustomValidity(w.message);
  const v=native.get.call(w.e);return{ok:owned(w.e)&&epochOf(w.e)===w.choice,reads:w.records,choice:w.choice,value:value.get.call(w.e),valid:v.valid,badInput:v.badInput,syntheticCustomValidity:w.invalid};
 };
 return{ok:true};
})()
