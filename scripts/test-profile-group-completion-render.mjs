import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const file = new URL("../src/desktop/profile_edit/sections/rich/search/render.js", import.meta.url);
const source = fs.readFileSync(file, "utf8");
const context = vm.createContext({Intl, localStorage: {getItem: () => null}});
vm.runInContext(source, context);
let count = 0;
const test = (name, body) => {body(); count++;};
const call = (name, input) => JSON.parse(JSON.stringify(context[name](input)));
const request = rows => ({ticket: "query-original", stage: 7, rows});
const row = (id, name, handle) => ({id, name, handle});
test("no-trim locale lower differs from ordinary member normalization", () => {
    const raw = "  I Cafe\u0301  ".replace("\\u0301", "\u0301");
    const result = call("profileGroupStrings", request([row("S00000001", raw, "handle")]));
    assert.equal(result.fields[0].lowered, raw.toLocaleLowerCase());
    assert.ok(result.fields[0].lowered.startsWith("  ") && result.fields[0].lowered.endsWith("  "));
    const old = call("profileChannelLower", {ticket: "query-original", stage: 7, query: raw, fields: [raw]});
    assert.equal(old.fields[0].localeLowered, raw.trim().toLocaleLowerCase());
    assert.equal(old.fields[0].lowered, raw.toLowerCase());
});
for (const [handle, expected] of [["", "@"], ["@", "@@"], ["@Alex Cafe\u0301", "@@Alex Caf\u00e9"],
    ["  Team  ", "@  Team  "], [null, "@null"], [false, "@false"], [0, "@0"]]) {
    test(`literal insertion handle ${JSON.stringify(handle)}`, () => {
        const result = call("profileGroupStrings", request([row("S00000001", "Name", handle)]));
        assert.equal(result.fields[0].label, expected);
        assert.deepEqual(result.inputs, [row("S00000001", "Name", handle)]);
    });
}
test("missing differs from explicit null", () => {
    const result = call("profileGroupStrings", request([{id:"S00000001", name:""}]));
    assert.equal(result.fields[0].label, "@undefined");
    assert.ok(!Object.hasOwn(result.inputs[0], "handle"));
});
test("distinct IDs with identical visible label preserve order", () => {
    const rows = [row("S00000002", "Second", "Same"), row("S00000001", "First", "Same")];
    const result = call("profileGroupStrings", request(rows));
    assert.deepEqual(result.fields.map(r => [r.id,r.label]), [["S00000002","@Same"],["S00000001","@Same"]]);
    assert.equal(result.ticket,"query-original"); assert.equal(result.stage,7);
    assert.equal(result.locale,Intl.DateTimeFormat().resolvedOptions().locale);
});
test("member and both group comparison fields share numeric collation equivalence", () => {
    const names = ["Member 2", "Group 12", "Group 02", "Group 2", "@Handle", "Actual Name"];
    const result = call("profileChannelCollation", {ticket:"same-query",names});
    const classes = new Map(result.classes.map(([index,rank]) => [names[index],rank]));
    assert.equal(classes.get("Group 02"),classes.get("Group 2"));
    assert.ok(classes.get("Group 2") < classes.get("Group 12"));
    assert.equal(classes.size,names.length);
});
for (const [name, input] of [
    ["null",null], ["missing ticket",{stage:0,rows:[]}], ["empty ticket",{ticket:"",stage:0,rows:[]}],
    ["negative stage",{ticket:"q",stage:-1,rows:[]}], ["fractional stage",{ticket:"q",stage:.1,rows:[]}],
    ["no rows",{ticket:"q",stage:0}], ["bad name",request([{id:"S1",name:null}])],
    ["bad handle object",request([row("S1","n",{})])], ["truthy number handle",request([row("S1","n",3)])],
    ["duplicate IDs",request([row("S1","a","a"),row("S1","b","b")])],
    ["extra row field",request([{id:"S1",name:"n",foreign:true}])],
    ["oversized batch",request(Array.from({length:129},(_,i)=>row(`S${i}`,"n","h")))],
]) test(`reject ${name}`,()=>assert.equal(call("profileGroupStrings",input),null));
test("empty batch remains observed with exact identity",()=>assert.deepEqual(call("profileGroupStrings",request([])),
    {ticket:"query-original",stage:7,locale:Intl.DateTimeFormat().resolvedOptions().locale,inputs:[],fields:[]}));
test("Turkish renderer locale remains untrimmed and is reported",()=>{
    const tr = vm.createContext({localStorage:{getItem:()=>null}});
    vm.runInContext(`const lower = String.prototype.toLocaleLowerCase; String.prototype.toLocaleLowerCase = function(){return lower.call(this,"tr-TR")}; Intl.DateTimeFormat = function(){return {resolvedOptions(){return {locale:"tr-TR"}}}};`,tr);
    vm.runInContext(source,tr);
    const result=tr.profileGroupStrings(request([row("S1"," I \u0130 ","Cafe\u0301")]));
    assert.equal(result.locale,"tr-TR");assert.equal(result.fields[0].lowered," \u0131 i ");
    assert.equal(result.fields[0].label,"@Caf\u00e9");
});
console.log(JSON.stringify({ok:true,checks:count,scope:"pure group string helper and shared collation; no native WebView claim"}));
