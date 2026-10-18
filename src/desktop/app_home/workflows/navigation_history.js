function workflowNavigationStorage(request, storage) {
  const key = request.key;
  if (typeof key !== "string" || !/^super-platinum\/[0-9a-f-]{36}\/client-ia-history$/i.test(key)) throw new Error("navigation owner");
  if (storage === undefined) {
    try { storage = globalThis.localStorage; } catch (_) { return {key,kind:request.write===null?"read-failed":"write-failed",value:null}; }
  }
  if (request.write !== null) {
    try { storage.setItem(key, JSON.stringify(request.write)); return {key,kind:"written",value:null}; }
    catch (_) { return {key,kind:"write-failed",value:null}; }
  }
  let value;
  try { value=storage.getItem(key); } catch (_) { return {key,kind:"read-failed",value:null}; }
  if (value === null) return {key,kind:"missing",value:null};
  try { return {key,kind:"parsed",value:JSON.parse(value)}; }
  catch (_) { return {key,kind:"invalid-json",value:null}; }
}
