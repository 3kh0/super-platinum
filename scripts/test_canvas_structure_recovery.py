"""Pure runner/SQLite rejection tests. Does not launch an application."""
from pathlib import Path
from contextlib import closing
import importlib.util,json,sqlite3,sys,tempfile,uuid
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parent))
source=Path(__file__).with_name('agent-canvas-structure-recovery-native.py')
spec=importlib.util.spec_from_file_location('canvas_recovery_runner',source);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
from native_check import expected_viewport
count=0
def rejects(action):
    global count
    try:action()
    except (RuntimeError,ValueError):count+=1
    else:raise AssertionError('invalid receipt was accepted')
with tempfile.TemporaryDirectory() as temp:
    db=Path(temp)/'receipt.sqlite'
    with closing(sqlite3.connect(db)) as c:
        c.executescript('PRAGMA user_version=2; CREATE TABLE identity(singleton INTEGER,account TEXT); CREATE TABLE receipts(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT);')
        c.execute('INSERT INTO identity VALUES(1,?)',(m.ACCOUNT,))
        c.commit()
    assert m.validate_stage(db,'initial')==[];count+=1
    body=dict(version=1,scope=dict(workspace='T1',user='U1',file='Ffixture'),thread='thread',document='document',target='target',session='abcdef123456',sequence=1,action={'Insert':dict(anchor='paragraph',before=False,position='aad:temp')},draft='Retain after restart',sections=[[0xc8,0x3e,0x81,0],[1],[2]],request=[1])
    def save(value):
        with closing(sqlite3.connect(db)) as c:
            c.execute('DELETE FROM receipts');c.execute('INSERT INTO receipts VALUES(?,?,?,?,?)',('T1','U1','Ffixture',str(uuid.uuid4()),json.dumps(value)))
            c.commit()
    save(body);original=m.validate_stage(db,'reserved-before-exit');count+=1
    assert m.validate_stage(db,'restored-locked',original)==original;count+=1
    rejects(lambda:m.validate_stage(db,'retired'))
    for key,value in [('version',2),('sequence',2),('scope',dict(workspace='T1',user='other',file='Ffixture')),('session','notanactor'),('sections',[[1],[2],[3]]),('draft','changed')]:
        save(dict(body,**{key:value}));rejects(lambda:m.validate_stage(db,'restored-locked'))
    save(dict(body,token='forbidden'));rejects(lambda:m.validate_stage(db,'restored-locked'))
    save(body);rejects(lambda:m.validate_stage(db,'restored-locked',original))
for mode in m.STAGES:
    spec=m.CanvasCheck('canvas-structure-recovery','flag',m.FLAGS[mode],m.COUNTS[mode],m.STAGES[mode])
    for fixture in m.FIXTURES:
        value=dict(ok=True,noSession=True,noTransport=True,layout=dict(ok=True,viewport=expected_viewport(fixture)),**m.COUNTS[mode],**dict.fromkeys(m.FLAGS[mode],True))
        m.validate_final(spec,fixture,value);count+=1
        for key in (*m.FLAGS[mode],'noSession','noTransport'):
            rejects(lambda key=key:m.validate(spec,fixture,dict(value,**{key:False})))
        for key in m.COUNTS[mode]:
            rejects(lambda key=key:m.validate_final(spec,fixture,dict(value,**{key:99})))
            for invalid in (bool(value[key]),float(value[key]),None):
                rejects(lambda key=key,invalid=invalid:m.validate_final(spec,fixture,dict(value,**{key:invalid})))
            rejects(lambda key=key:m.validate_final(spec,fixture,{k:v for k,v in value.items() if k!=key}))
for name, expected in m.STAGE_COUNTS.items():
    value=dict(stage=name,**dict(zip(('dispatches','reservations','retirements'),expected)))
    m.validate_counts(value);count+=1
    for key in ('dispatches','reservations','retirements'):
        rejects(lambda key=key:m.validate_counts(dict(value,**{key:99})))
        for invalid in (bool(value[key]),float(value[key]),None):
            rejects(lambda key=key,invalid=invalid:m.validate_counts(dict(value,**{key:invalid})))
        rejects(lambda key=key:m.validate_counts({k:v for k,v in value.items() if k!=key}))

with tempfile.TemporaryDirectory() as temp:
    log=Path(temp)/'closed.log'
    class Closed:
        pass
    closed=Closed();closed.log=log
    stages=[dict(stage='initial')];final=dict(ok=True)
    def write_log(stages,finals):
        log.write_text(''.join('canvas-structure-recovery-stage: '+json.dumps(s)+'\n' for s in stages)+''.join('canvas-structure-recovery-check: '+json.dumps(f)+'\n' for f in finals),encoding='utf8')
    write_log(stages,[final]);m.validate_closed(closed,stages,final);count+=1
    write_log(stages+[dict(stage='late')],[final]);rejects(lambda:m.validate_closed(closed,stages,final))
    write_log(stages,[final,final]);rejects(lambda:m.validate_closed(closed,stages,final))
    log.write_text('canvas-structure-recovery-check: {"ok":true}',encoding='utf8')
    rejects(lambda:m.validate_closed(closed,stages,final))

failure={"ok":False,"error":"Saved canvas changes could not be checked. Refresh before editing."}
evidence={}
try:m.record_terminal(evidence,[failure],'seed',1)
except RuntimeError as error:assert str(error)=='Native recovery check failed: '+failure['error'];count+=1
else:raise AssertionError('native failure accepted')
assert evidence['native']==failure and evidence['nativeReceipts']==[failure];count+=1
rejects(lambda:m.record_terminal({},[failure,failure],'seed',1))
rejects(lambda:m.record_terminal({},[dict(ok=True,mode='restart')],'seed',5))
rejects(lambda:m.record_terminal({},[dict(ok=True,mode='seed')],'seed',1))
m.record_terminal({},[dict(ok=True,mode='seed')],'seed',5);count+=1

# Exercise phase error recording using an injected owned context. This does not
# certify the separately tested native audit implementation or launch a process.
from contextlib import contextmanager
class Process:
    pid=1234
    alive=True
    def poll(self):return None if self.alive else 7
class FakeNative:
    def __init__(self,fixture,log,binary,checks):
        self.log=log;self.process=Process()
        log.write_text('canvas-structure-recovery-check: '+json.dumps(failure)+'\n',encoding='utf8')
    def require_running(self):assert self.process.alive
    def receipts(self,prefix):return m.closed_receipts(self.log,prefix)
@contextmanager
def audit(native,evidence):
    try:yield object()
    finally:
        native.process.alive=False
        evidence.update(processExited=True,exitCode=7,cleanupErrors=[])
saved_native,saved_audit=m.NativeFixture,m.audited_fixture
try:
    m.NativeFixture,m.audited_fixture=FakeNative,audit
    with tempfile.TemporaryDirectory() as temp:
        captures=Path(temp)
        try:m.phase('canvas-structure','seed',captures/'unused.sqlite',captures/'unused.exe',captures)
        except RuntimeError as error:assert str(error)=='Native recovery check failed: '+failure['error'];count+=1
        else:raise AssertionError('failed phase succeeded')
        result=json.loads((captures/'canvas-structure-recovery-seed.json').read_text())
        assert result['ok'] is False and result['native']==failure;count+=1
        assert result['processExited'] is True and result['exitCode']==7 and result['cleanupErrors']==[];count+=1
        assert result['closedFinals']==[failure] and result['closedStages']==[];count+=1
finally:m.NativeFixture,m.audited_fixture=saved_native,saved_audit
with tempfile.TemporaryDirectory() as temp:
    temp=Path(temp);closed=Closed();closed.log=temp/'closed.log';output=temp/'phase.json'
    stages=[dict(stage='initial')];final=dict(ok=True)
    closed.log.write_text('canvas-structure-recovery-stage: '+json.dumps(stages[0])+'\ncanvas-structure-recovery-check: '+json.dumps(final)+'\n',encoding='utf8')
    value=dict(ok=True,native=final)
    m.finalize_phase(closed,value,output,stages)
    assert value['closedLogRevalidated'] is True and json.loads(output.read_text())['ok'] is True;count+=1
    closed.log.write_text('canvas-structure-recovery-check: {"ok":true}',encoding='utf8')
    value=dict(ok=True,native=final)
    rejects(lambda:m.finalize_phase(closed,value,output,stages))
    assert value['ok'] is False and 'closedLogError' in value and json.loads(output.read_text())['ok'] is False;count+=1
    value=dict(ok=False,native=failure,error='original native failure')
    m.finalize_phase(closed,value,output,stages)
    assert value['error']=='original native failure' and 'closedLogError' in value;count+=1
    closed.log.write_text('canvas-structure-recovery-check: '+json.dumps(final)+'\n',encoding='utf8')
    value=dict(ok=True,native=final)
    rejects(lambda:m.finalize_phase(closed,value,output,stages))
    assert value['ok'] is False and 'closedLogRevalidated' not in value;count+=1
with tempfile.TemporaryDirectory() as temp:
    database=Path(temp)/'review.sqlite'
    original=[dict(workspace='T1',user='U1',file='Ffixture',operation=str(uuid.uuid4()),body=json.dumps(body))]
    with closing(sqlite3.connect(database)) as c:
        c.executescript('PRAGMA user_version=2; CREATE TABLE identity(singleton INTEGER,account TEXT); CREATE TABLE receipts(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT); CREATE TABLE local_reviews(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT,observed TEXT,reviewed_at INTEGER,decision TEXT);')
        c.execute('INSERT INTO identity VALUES(1,?)',(m.ACCOUNT,))
        c.execute('INSERT INTO receipts VALUES(?,?,?,?,?)',tuple(original[0][k] for k in m.COLUMNS));c.commit()
    for name in ('review-unchecked','review-controls-unchecked','review-confirmation'):
        assert m.validate_stage(database,name,original)==original;count+=1
    rejects(lambda:m.validate_stage(database,'review-removed',original))
    observed=dict(thread_sequence=1000,document_sequence=1000,sections=body['sections'])
    with closing(sqlite3.connect(database)) as c:
        c.execute('DELETE FROM receipts')
        c.execute('INSERT INTO local_reviews VALUES(?,?,?,?,?,?,?,?)',(*tuple(original[0][k] for k in m.COLUMNS),json.dumps(observed),123,'local_unlock_outcome_unknown'));c.commit()
    for name in ('review-removed','review-refreshed','review-decision-retained'):
        assert len(m.validate_stage(database,name,original))==1;count+=1
    rejects(lambda:m.validate_stage(database,'review-unchecked',original))
    for column,value in [('decision','accepted'),('body',original[0]['body']+' '),('reviewed_at',0),('observed',json.dumps(dict(observed,sections=[]))),('observed',json.dumps(dict(observed,document_sequence=1000.0)))]:
        with closing(sqlite3.connect(database)) as c:
            previous=c.execute('SELECT '+column+' FROM local_reviews').fetchone()[0]
            c.execute('UPDATE local_reviews SET '+column+'=?',(value,));c.commit()
        rejects(lambda:m.validate_stage(database,'review-refreshed',original))
        with closing(sqlite3.connect(database)) as c:
            c.execute('UPDATE local_reviews SET '+column+'=?',(previous,));c.commit()
    # Run the actual phase orchestration against the archived SQLite fixture.
    # This catches accidentally dropping the seed row when passing stage proof.
    class ArchivedNative:
        def __init__(self,fixture,log,binary,checks):
            self.log=log;self.process=Process();self.process.alive=True
            self.stage=dict(ok=True,stage='review-decision-retained',ordinal=0,noSession=True,noTransport=True,**m.COUNTS['reviewed'])
            self.final=dict(ok=True,mode='reviewed',noSession=True,noTransport=True,layout=dict(ok=True,viewport=expected_viewport(fixture)),**m.COUNTS['reviewed'],**dict.fromkeys(m.FLAGS['reviewed'],True))
            log.write_text('canvas-structure-recovery-stage: '+json.dumps(self.stage)+'\ncanvas-structure-recovery-check: '+json.dumps(self.final)+'\n',encoding='utf8')
        def require_running(self):assert self.process.alive
        def receipts(self,prefix):return m.closed_receipts(self.log,prefix)
    class Capture:
        def capture(self,path,label):pass
    @contextmanager
    def archived_audit(native,evidence):
        try:yield Capture()
        finally:native.process.alive=False
    old=(m.NativeFixture,m.audited_fixture,m.capture_stage)
    try:
        m.NativeFixture,m.audited_fixture=ArchivedNative,archived_audit
        m.capture_stage=lambda *a,**kw:dict(stage=a[3]['stage'])
        result=m.phase('canvas-local-review','reviewed',database,Path(temp)/'unused.exe',Path(temp),original)
        assert result['ok'] is True and result['closedLogRevalidated'] is True;count+=1
        assert result['database']==m.validate_stage(database,'review-decision-retained',original);count+=1
    finally:m.NativeFixture,m.audited_fixture,m.capture_stage=old
# Strict geometry proof uses actual control rectangles, clip intersections and
# exact checkbox/button states; a broad form center cannot substitute for them.
from copy import deepcopy
for fixture in ('canvas-local-review','canvas-local-review-narrow'):
    viewport=expected_viewport(fixture)
    for name,checked in (('review-controls-unchecked',False),('review-confirmation',True)):
        item=dict(rect=[30,140,60,160],clip=[20,120,viewport[0]-20,viewport[1]-10],point=[45,150],visible=True,hit=True,hitTested=True)
        proof=dict(ok=True,checked=checked,submitDisabled=not checked,cancelDisabled=False,checkboxDisabled=False,viewport=viewport,
                   checkbox=deepcopy(item),label=dict(item,hitTested=False),cancel=deepcopy(item),remove=deepcopy(item))
        packet=dict(stage=name,reviewGeometry=proof)
        m.validate_review_geometry(packet,fixture);count+=1
        rejects(lambda:m.validate_review_geometry(dict(stage=name),fixture))
        for key in ('ok','checked','submitDisabled','cancelDisabled','checkboxDisabled'):
            bad=deepcopy(packet);bad['reviewGeometry'][key]=int(proof[key]);rejects(lambda:m.validate_review_geometry(bad,fixture))
        for control in ('checkbox','label','cancel','remove'):
            for field,value in [('visible',False),('hit',False),('hitTested',control=='label'),('rect',[30,100,60,160]),('clip',[20,120,viewport[0]+1,viewport[1]-10]),('point',[31,150]),('rect',[True,140,60,160]),('rect',[float('nan'),140,60,160])]:
                bad=deepcopy(packet);bad['reviewGeometry'][control][field]=value;rejects(lambda:m.validate_review_geometry(bad,fixture))
        bad=deepcopy(packet);bad['reviewGeometry']['viewport']=[1,1];rejects(lambda:m.validate_review_geometry(bad,fixture))
print(f'{count} pure recovery runner/SQLite checks passed; no native process launched.')
