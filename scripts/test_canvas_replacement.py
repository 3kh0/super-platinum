"""Pure protocol/SQLite tests, synthetic bytes; no renderer or Rust execution."""
import copy, importlib.util, json, sqlite3, sys, tempfile, uuid
from pathlib import Path
from contextlib import closing
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parent))
sys.path.append(str(Path.cwd()/'scripts'))
spec=importlib.util.spec_from_file_location('replacement_runner',Path(__file__).with_name('agent-canvas-replacement-native.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
from native_check import expected_viewport
count=0
def rejects(fn):
    global count
    try:fn()
    except (RuntimeError,ValueError):count+=1
    else:raise AssertionError('invalid input admitted')
def varint(n):
    out=[]
    while n>=128:out.append((n&127)|128);n>>=7
    return bytes(out+[n])
def n(tag,v):return varint(tag<<3)+varint(v)
def b(tag,v):
    v=v.encode() if isinstance(v,str) else bytes(v)
    return varint((tag<<3)|2)+varint(len(v))+v
def body():
    target='temp:C:xyz'+'a'*25;new='temp:C:xyz'+'b'*25
    title=b(1,'title')+n(2,1000)+b(6,'thread')+b(7,'document')+bytes([0xc8,0x3e,0x81,0])
    old=b(1,target)+n(2,2000)+b(6,'thread')+b(7,'document')
    def update(id,deleted):return b(1,id)+n(3,deleted)+n(19,1)+n(20,1)+b(33,'abcdef123456-0')+b(38,n(2,0)+b(6,b'head'))
    request=b(1,update(target,1))+b(1,update(new,0)+b(8,'aad:temp'))+b(14,b(1,'title'))
    return dict(version=2,scope=dict(workspace='T1',user='U1',file='Ffixture'),thread='thread',document='document',target=target,session='abcdef123456',sequence=1,action={'Replace':dict(replacement=new,position='aad:temp')},draft='Replacement <b>edited</b>',sections=[list(title),list(old)],request=list(request))
value=body();m.validate_body(value);count+=1
for key,v in [('version',1),('version',True),('sequence',True),('sequence',1.0),('thread','other'),('document','other'),('target',value['action']['Replace']['replacement']),('sections',value['sections'][:1]),('request',[1]),('session','foreign'),('action',{'Insert':{}})]:
    rejects(lambda key=key,v=v:m.validate_body(dict(value,**{key:v})))
for key in value:
    rejects(lambda key=key:m.validate_body({k:v for k,v in value.items() if k!=key}))
rejects(lambda:m.validate_body(dict(value,token='fixture-only-invalid-key')))
for malformed in ([0],[255],[10,5,1],[9,1],[15],[8,255],[],[True],[256],[-1]):rejects(lambda malformed=malformed:m.fields(malformed))
for tag in (19,20,33,38):
    fields=m.fields(value['request']); updates=[m.fields(v) for t,w,v in fields if t==1]
    encoded=b(1,b''.join(n(t,v) if w==0 else b(t,v) for t,w,v in updates[0] if t!=tag))+b(1,fields[1][2])+b(14,fields[2][2])
    rejects(lambda encoded=encoded:m.validate_body(dict(value,request=list(encoded))))

with tempfile.TemporaryDirectory() as temp:
    db=Path(temp)/'test.sqlite'
    with closing(sqlite3.connect(db)) as c:
        c.executescript('PRAGMA user_version=2; CREATE TABLE identity(singleton INTEGER,account TEXT); CREATE TABLE receipts(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT); CREATE TABLE local_reviews(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT,observed TEXT,reviewed_at INTEGER,decision TEXT);')
        c.execute('INSERT INTO identity VALUES(1,?)',(m.ACCOUNT,));c.commit()
    assert m.validate_stage(db,'initial')==[];count+=1
    with closing(sqlite3.connect(db)) as c:
        c.execute('INSERT INTO receipts VALUES(?,?,?,?,?)',('T1','U1','Ffixture',str(uuid.uuid4()),json.dumps(value)));c.commit()
    original=m.validate_stage(db,'uncertain-before-exit');count+=1
    assert m.validate_stage(db,'restored-locked',original)==original;count+=1
    rejects(lambda:m.validate_stage(db,'initial'))
    rejects(lambda:m.validate_stage(db,'unknown'))
    observed=sorted(value['sections'],key=lambda raw:m.one(m.fields(raw),1,2))
    for name in m.STAGES['review'][:3]:assert m.validate_stage(db,name,original,observed)==original;count+=1
    with closing(sqlite3.connect(db)) as c:
        c.execute('INSERT INTO local_reviews SELECT *,?,?,? FROM receipts',(json.dumps(dict(thread_sequence=1000,document_sequence=1000,sections=observed)),100,'local_unlock_outcome_unknown'))
        c.execute('DELETE FROM receipts');c.commit()
    for name in (*m.STAGES['review'][3:],*m.STAGES['reviewed']):assert len(m.validate_stage(db,name,original,observed))==1;count+=1
    rejects(lambda:m.validate_stage(db,'review-decision-retained',original,None))
    rejects(lambda:m.validate_stage(db,'review-decision-retained',original,list(reversed(observed))))
    with closing(sqlite3.connect(db)) as c:c.execute("UPDATE local_reviews SET decision='success'");c.commit()
    rejects(lambda:m.validate_stage(db,'review-decision-retained',original,observed))

def action_records():
    labels=['Delete paragraph','Saving\u2026','Saving\u2026','Checking\u2026','Delete paragraph','Unconfirmed']
    return [dict(ok=True,label=label,generation='7' if i<4 else '8',visit='3',groupWidth=400,cancel=[180,0,72,32],save=[260,0,140,32]) for i,label in enumerate(labels)]

for mode in m.STAGES:
    spec=m.CanvasCheck('canvas-replacement','flag',m.FLAGS[mode],m.COUNTS[mode],m.STAGES[mode])
    for fixture in m.FIXTURES:
        value=dict(ok=True,noSession=True,noTransport=True,layout=dict(ok=True,viewport=expected_viewport(fixture)),**m.COUNTS[mode],**dict.fromkeys(m.FLAGS[mode],True))
        if mode=="seed":value["actionGeometry"]=action_records()
        m.validate_final(spec,fixture,value);count+=1
        for key in (*m.FLAGS[mode],'noSession','noTransport'):
            rejects(lambda key=key:m.validate_final(spec,fixture,dict(value,**{key:False})))
        for key in m.COUNTS[mode]:
            for invalid in (99,bool(value[key]),float(value[key]),None):
                rejects(lambda key=key,invalid=invalid:m.validate_final(spec,fixture,dict(value,**{key:invalid})))
            rejects(lambda key=key:m.validate_final(spec,fixture,{k:v for k,v in value.items() if k!=key}))
for name,expected in m.STAGE_COUNTS.items():
    value=dict(stage=name,**dict(zip(('dispatches','reservations','retirements','textAttempts'),expected)))
    m.validate_counts(value);count+=1
    for key in ('dispatches','reservations','retirements','textAttempts'):
        for invalid in (99,bool(value[key]),float(value[key]),None):
            rejects(lambda key=key,invalid=invalid:m.validate_counts(dict(value,**{key:invalid})))
        rejects(lambda key=key:m.validate_counts({k:v for k,v in value.items() if k!=key}))
with tempfile.TemporaryDirectory() as temp:
    class Native:pass
    native=Native();native.log=Path(temp)/'closed.log'
    stages=[dict(stage='initial')];final=dict(ok=True)
    def log(s,f):native.log.write_text(''.join('canvas-replacement-stage: '+json.dumps(v)+'\n' for v in s)+''.join('canvas-replacement-check: '+json.dumps(v)+'\n' for v in f),encoding='utf-8')
    log(stages,[final]);m.validate_closed(native,stages,final);count+=1
    log(stages+stages,[final]);rejects(lambda:m.validate_closed(native,stages,final))
    log(stages,[final,final]);rejects(lambda:m.validate_closed(native,stages,final))
    native.log.write_text('canvas-replacement-check: {"ok":true}',encoding='utf-8')
    rejects(lambda:m.validate_closed(native,stages,final))
    evidence={'ok':True,'native':final};path=Path(temp)/'result.json'
    rejects(lambda:m.finalize_phase(native,evidence,path,stages));assert evidence['ok'] is False;count+=1
for fixture in m.FIXTURES:
    width,height=expected_viewport(fixture)
    geometry={'ok':True,'viewport':[width,height],**{key:dict(ok=True,hit=True,rect=[10,10,100,30],clip=dict(left=0,top=0,right=width,bottom=height)) for key in ('hint','cancel','save')}}
    packet=dict(stage='confirmation',reviewGeometry=geometry)
    m.validate_review_geometry(packet,fixture);count+=1
    for key in ('hint','cancel','save'):
        for field,value in [('ok',False),('hit',False),('rect',[0,0,0,30]),('rect',[0,-50,20,30]),('rect',[0,0,True,30]),('rect',[float('nan'),0,20,30]),('clip',dict(left=0,top=0,right=5,bottom=height))]:
            altered=copy.deepcopy(packet);altered['reviewGeometry'][key][field]=value
            rejects(lambda altered=altered:m.validate_review_geometry(altered,fixture))
m.validate_action_geometry(action_records());count+=1
for bad in (None,[],action_records()[:-1]):rejects(lambda bad=bad:m.validate_action_geometry(bad))
for index in range(6):
    for key,value in [('ok',False),('label','wrong'),('generation',''),('visit',None),('groupWidth',True),('groupWidth',float('nan')),('cancel',[180,0,0,32]),('save',[260,0,141,32]),('save',[260,0,True,32])]:
        bad=action_records();bad[index][key]=value
        rejects(lambda bad=bad:m.validate_action_geometry(bad))
for index in (1,2,3,5):
    for key in ('cancel','save'):
        for coord in range(4):
            bad=action_records();bad[index][key][coord]-=1
            rejects(lambda bad=bad:m.validate_action_geometry(bad))
    bad=action_records();bad[index]['generation']='99'
    rejects(lambda bad=bad:m.validate_action_geometry(bad))
bad=action_records();bad[4]['generation']=bad[5]['generation']='7'
rejects(lambda:m.validate_action_geometry(bad))
print(f'{count} paired runner/SQLite cases PASS (synthetic schema bytes; no native/Cargo)')
