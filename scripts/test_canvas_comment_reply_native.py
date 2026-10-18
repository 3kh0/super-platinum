import sys
sys.dont_write_bytecode=True
from pathlib import Path
import copy
import importlib.util
import tempfile
import unittest
import uuid
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
# During TMP verification shared audited helpers remain the actual repository copies.
ROOT=next(p for p in HERE.parents if (p/"Cargo.toml").is_file() and (p/"scripts/native_fixture.py").is_file())
sys.path.append(str(ROOT/"scripts"))
spec=importlib.util.spec_from_file_location("reply_runner",HERE/"check-canvas-comment-reply-native.py")
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def uid(n):return str(uuid.UUID(int=n))
def rect(x=10,y=10,w=100,h=32):return dict(x=x,y=y,width=w,height=h,right=x+w,bottom=y+h)
def layout(text,disabled=False):
    return dict(ok=True,viewport=m.expected_viewport(m.FIXTURE),noOverflow=True,sampledSequentially=True,dialog=rect(w=500,h=300),panel=rect(w=450,h=250),finalRect=rect(),finalTarget="canvas-comment-reply",text=text,sendDisabled=disabled,samples=[dict(id=i,label=i,rect=rect(),hit=True,disabled=False) for i in ("canvas-comment-reply","canvas-reply-send","history")])
def draft(text,n):return dict(id=uid(n+100),capture=uid(n+200),revision=1,text=text)
def receipt(text,n):
 d=draft(text,n);return dict(id=uid(n),draft=d,record=dict(version=1,workspace="T1",user="U0",file="Ffixture",root=dict(type="message",ts=m.ROOT_TS,subtype="document_comment_root",document_comment=dict(thread_id="fixture-comment-1")),text=text,client_msg_id=uid(n)))
def req(method,form):return dict(method="POST",url="https://fixture.slack.com/api/"+method+"?_x_version_ts=fixture",fields=[list(p) for p in form.items()])
def write(text,n):return req("chat.postMessage",dict(channel="Cfixture",text=text,thread_ts=m.ROOT_TS,client_msg_id=uid(n),reply_broadcast="false"))
def sample(phase="exercise"):
 old=receipt(m.UNKNOWN,3)
 history=[dict(receipt=receipt(m.FIRST,1),status="not_sent",observed=None),dict(receipt=receipt(m.FIRST,2),status="acknowledged",observed={"ts":"1719810001.000100"})]
 unknown=dict(drafts=[old["draft"]],attempt=old,history=history)
 initial=dict(drafts=[],attempt=None,history=[]) if phase=="exercise" else copy.deepcopy(unknown)
 final=unknown if phase=="exercise" else dict(drafts=[draft("",9)],attempt=None,history=history+[dict(receipt=old,status="reviewed_outcome_unknown",observed=old["record"]["root"]),dict(receipt=receipt(m.FINAL,4),status="acknowledged",observed={"ts":"1719810001.000100"})])
 prepared=[write(m.FIRST,1),write(m.FIRST,2),write(m.UNKNOWN,3)] if phase=="exercise" else [write(m.FINAL,4)]
 writes=iter(prepared[1:] if phase=="exercise" else prepared)
 requests=[next(writes) if method=="chat.postMessage" else req(method,m.read_form(method)) for method in m.REQUESTS[phase]]
 texts=[m.FIRST,m.FIRST,m.FIRST,m.NEWER,m.NEWER,m.NEWER,m.UNKNOWN] if phase=="exercise" else [m.UNKNOWN,"",""]
 layouts=[layout(t,(phase=="exercise" and i==6) or (phase=="restart" and i==0)) for i,t in enumerate(texts)]
 return dict(ok=True,phase=phase,syntheticInput=True,noSession=True,noTransport=True,durableDraft=True,exactRequests=True,unknownNoReplay=True,preparedCount=len(prepared),dispatchCount=len(prepared)-(1 if phase=="exercise" else 0),initial=initial,final=final,ledger=dict(prepared=prepared,requests=requests),layouts=layouts,layout=layouts[-1])
class Validation(unittest.TestCase):
 def test_both_faithful_phases(self):
  for phase in m.STAGES:m.validate(m.FIXTURE,phase,sample(phase))
 def test_restarted_lock_is_required(self):
  r=sample("restart");r["layouts"][0]["sendDisabled"]=False
  with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"restart",r)
 def test_exact_raw_payload_and_no_broadcast(self):
  for key,value in (("text","other"),("thread_ts","1.000100"),("reply_broadcast","true"),("channel","Cother")):
   r=sample();r["ledger"]["prepared"][0]["fields"]=[(k,value if k==key else v) for k,v in r["ledger"]["prepared"][0]["fields"]]
   with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_no_read_route_or_query_injection(self):
  for alter in (lambda r:r["ledger"]["requests"][0]["fields"].append(["cursor","foreign"]),lambda r:r["ledger"]["requests"][0].update(url="https://other.slack.com/api/conversations.history")):
   r=sample();alter(r)
   with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_inventory_and_identity_are_not_flags(self):
  for alter in (lambda r:r["final"].pop("history"),lambda r:r["final"]["attempt"].update(id=uid(99)),lambda r:r["final"]["attempt"]["record"]["root"].update(ts="2.000100")):
   r=sample();alter(r)
   with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_review_preserves_original_and_prior_history(self):
  for index in (0,2):
   r=sample("restart");r["final"]["history"][index]["receipt"]["draft"]["text"]="changed"
   with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"restart",r)
 def test_geometry_recomputed_and_false_hit_refused(self):
  for mutate in (lambda g:g["samples"][0].update(hit=False),lambda g:g["dialog"].update(right=float("nan")),lambda g:g["samples"][1]["rect"].update(x=-100,right=0),lambda g:g.update(samples=[])):
   r=sample();mutate(r["layouts"][0])
   with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_newer_text_is_not_replaced_by_sent_snapshot(self):
  r=sample();r["layouts"][4]["text"]=m.FIRST
  with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_request_replay_or_duplicate_id_refused(self):
  r=sample();r["ledger"]["requests"].append(r["ledger"]["prepared"][0])
  with self.assertRaises(RuntimeError):m.validate(m.FIXTURE,"exercise",r)
 def test_evidence_io_keeps_primary_failure(self):
  with tempfile.TemporaryDirectory() as d:
   primary=RuntimeError("workflow failed");m.persist_evidence(Path(d)/"absent"/"receipt.json",{},primary);self.assertEqual(str(primary),"workflow failed");self.assertTrue(primary.__notes__)
if __name__=="__main__":unittest.main()
