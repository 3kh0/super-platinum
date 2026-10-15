"""Pure evidence contract tests; no app, account, HTTP, or native keyboard."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch
R=runpy.run_path(str(Path(__file__).with_name('check-emoji-native.py')))
from test_multi_external_select_receipts import png

def proof(text='',disabled=False,bounds=None):
    b=bounds or dict(x=10,y=10,width=120,height=30)
    return dict(rect=b,visible=True,unobscured=True,disabled=disabled,text=text,focused=False,
        points=[dict(x=b['x']+x*b['width'],y=b['y']+y*b['height'],owned=True,hit=dict(tag='BUTTON',id='',classes='')) for x in (.15,.5,.85) for y in (.15,.5,.85)])
def owned():return dict(account='emoji-native-disconnected',epoch=1,team='T1',channel='C2',view=1,channelGeneration=1,scope='S1',mount='M1',catalog='1/local',preferences=dict(loaded=True,revision=1,raw={}))
def native(text,o,start=None,end=None,direction='none'):
    n=R['units'](text);return dict(text=text,codePoints=[ord(c) for c in text],utf16Length=n,start=n if start is None else start,end=n if end is None else end,direction=direction,
        scope=o['scope'],visit=o['mount'],catalog=o['catalog'],preferences=str(o['preferences']['revision']),focused=True,composing=False)
def click(viewport,disabled=False):return dict(ok=True,control=proof(disabled=disabled),clickCalls=2,viewport=viewport,documentWidth=viewport[0])
def opened(viewport):return dict(ok=True,control=proof(),clicked=True,viewport=viewport,documentWidth=viewport[0])

class Receipts(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup);self.dir=Path(temp.name);self.nonce='a'*32;self.pid=42
    def good(self,fixture='emoji-native-narrow'):
        v=R['expected_viewport'](fixture);o=owned();stage_owners={};actions=[];events=[]
        def event(stage,kind,value):events.append(dict(sequence=len(events),stage=stage,kind=kind,value=copy.deepcopy(value)))
        def complete_events(a,success):
            stage=a['kind'];b=a['before'];snapshot=R['wire_native'](b)
            ticket=dict(serial=1,seq=len(events)+1,owner=b['scope']+':mount:1',catalog=b['catalog'],choice='choice-'+stage,snapshot=snapshot)
            event(stage,'worker-present',dict(seq=ticket['seq'],owner=ticket['owner'],snapshot=snapshot,catalog=b['catalog'],preferences=int(b['preferences']),choices=[dict(id=ticket['choice'],label='rocket',detail='')]))
            event(stage,'completion-choose',dict(ticket=ticket))
            if success or stage in ('guard-newer','guard-composition'):
                answer={k:ticket[k] for k in ('serial','seq','owner','catalog','choice')}
                answer.update(kind='emoji_admit',allowed=True,value=':party_fixture:' if stage=='completion-custom' else '\U0001f680',start=3,end=b['end'],spacer=' ')
                event(stage,'completion-admission',dict(answer=answer))
                if success:event(stage,'completion-ack',dict(ticket=ticket,after=R['wire_native'](a['after']),validated=True,model=a['after']['text']))
            elif stage=='guard-catalog':event(stage,'completion-admission',dict(answer=None))
        for stage,text,after in [('completion-standard',R['QUERY'],R['STANDARD']),('completion-custom',R['CUSTOM'],R['CUSTOM_INSERTED'])]:
            a=dict(kind=stage,choice=click(v),before=native(text,o),after=native(after,o,direction='forward'));actions.append(a);complete_events(a,True)
        before=native(R['BASE'],o,3,10,'backward')
        actions.append(dict(kind='picker-cancelled',open=opened(v),escape=dict(ok=True,prevented=True),before=before,after=copy.deepcopy(before)))
        event('picker-cancelled','picker-open',dict(serial=1,snapshot=R['wire_native'](before)))
        a=dict(kind='picker-inserted',open=opened(v),blocked=click(v,True),choice=click(v),before=copy.deepcopy(before),after=native(R['PICKED'],o,5,5,'forward'))
        actions.append(a)
        event(a['kind'],'picker-open',dict(serial=2,snapshot=R['wire_native'](before)))
        event(a['kind'],'picker-choose',dict(serial=2,snapshot=R['wire_native'](before),value='\U0001f680',guard=dict(catalog=o['catalog'],preferences='1')))
        event(a['kind'],'picker-ack',dict(serial=2,confirmed=True,scopeCurrent=True,catalogCurrent=True))
        for stage in R['STAGES'][:10]:stage_owners[stage]=copy.deepcopy(o)
        stage_owners['picker-unready']['preferences']=dict(loaded=False,revision=0,raw={})
        for stage in R['STAGES'][10:]:
            b=copy.deepcopy(o);before=native(R['QUERY'],b)
            if stage=='guard-catalog':o['catalog']='2/local'
            elif stage=='guard-account':o.update(epoch=2,scope='S2',mount='M2',catalog='3/local')
            elif stage=='guard-mount':o.update(view=2,scope='S3',mount='M3')
            elif stage=='guard-navigation':o.update(channelGeneration=3,scope='S4',mount='M4')
            a=dict(kind=stage,choice=click(v),before=before,after=native(R['NEWER'] if stage=='guard-newer' else R['QUERY'],o,direction='forward' if stage=='guard-newer' else 'none'),ownerBefore=b,ownerAfter=copy.deepcopy(o),
                composition=dict(ok=True,type='compositionstart',data='',bubbles=True,isTrusted=False) if stage=='guard-composition' else None)
            actions.append(a);complete_events(a,False);stage_owners[stage]=copy.deepcopy(o)
        texts={s:R['QUERY'] for s in R['STAGES']};texts.update(initial=R['BASE'],**{'completion-standard':R['STANDARD'],'custom-offered':R['CUSTOM'],'completion-custom':R['CUSTOM_INSERTED'],'picker-selected':R['BASE'],'picker-cancelled':R['BASE'],'picker-unready':R['BASE'],'picker-ready':R['BASE'],'picker-inserted':R['PICKED'],'guard-newer':R['NEWER']})
        geometry=[];captures={}
        for stage in R['STAGES']:
            surface='completion' if stage in ('completion-offered','custom-offered') else 'picker' if stage in ('picker-selected','picker-unready','picker-ready') else 'composer'
            names=('panel','query','choice') if surface=='picker' else ('field','footer','popup','choice') if surface=='completion' else ('field','footer')
            controls={name:proof() for name in names}
            if surface=='picker':controls['choice']['disabled']=stage=='picker-unready'
            text=texts[stage]
            geometry.append(dict(ok=True,stage=stage,surface=surface,controls=controls,native=native(text,stage_owners[stage]),viewport=v,documentWidth=v[0],pickerReady=stage!='picker-unready' if surface=='picker' else None,pickerNode=('1' if stage=='picker-selected' else '2') if surface=='picker' else None,
                owner=stage_owners[stage],model=dict(text=text,selection=dict(start=len(text.encode()),end=len(text.encode())),reply_broadcast=False)))
            path=self.dir/(stage+'.png');path.write_bytes(png());captures[stage]=str(path)
        controller=[]
        for stage in ('completion-standard','completion-custom','guard-newer','guard-catalog','composition-start','guard-composition','composition-end'):
            name='guard-composition' if stage.startswith('composition-') else stage
            a=next(a for a in actions if a['kind']==name);n=a['after']
            t=next(e['value']['ticket'] for e in events if e['stage']==name and e['kind']=='completion-choose')
            decision=not stage.startswith('composition-');result={k:t[k] for k in ('serial','seq','owner','catalog','choice')}
            result['allowed']=stage!='guard-catalog'
            controller.append(dict(sequence=len(controller),kind='admission' if decision else stage,owner=t['owner'],scope=n['scope'],mount=n['visit'],composing=stage in ('composition-start','guard-composition'),focused=True,snapshot=R['wire_native'](n),result=result if decision else None,accepted=stage.startswith('completion-') if decision else None))
        messages=dict(channels={'C2':dict(messages=[],pending=[])},threadCount=0,pendingDeliveryCount=0)
        return dict(ok=True,fixture=fixture,nonce=self.nonce,pid=self.pid,viewport=v,noSession=True,noTransport=True,noCache=True,preparedRequests=0,attempts=[],syntheticInput=True,syntheticPreferences=True,preferenceWriterCovered=False,
            geometry=geometry,captures=captures,actions=actions,events=events,controllerEvents=controller,messagesInitial=messages,messagesFinal=copy.deepcopy(messages),limitations=['synthetic','no HTTP','no writer','channel only'])
    def validate(self,v):R['validate'](v['fixture'],self.nonce,self.pid,v)
    def reject(self,v):
        with self.assertRaises(RuntimeError):self.validate(v)
    def test_full_native_contract_both_sizes(self):
        for fixture in R['FIXTURES']:self.validate(self.good(fixture))
    def test_staged_runner_cannot_launch(self):
        with patch.dict(R['contract'].__globals__,CONTRACT_READY=False):
            with self.assertRaises(RuntimeError):R['run'](R['FIXTURE'],None,None)
    def test_flags_counts_and_owner_are_strict_types(self):
        for key in ('ok','noSession','noTransport','noCache','preparedRequests','syntheticInput','preferenceWriterCovered','pid'):
            v=self.good();v[key]=not v[key] if type(v[key]) is bool else True;self.reject(v)
        v=self.good();v['attempts']=[dict(method='POST')];self.reject(v)
    def test_ack_requires_worker_choice_and_actual_successful_admission(self):
        for kind in ('worker-present','completion-choose','completion-admission'):
            v=self.good();index=next(i for i,e in enumerate(v['events']) if e['kind']==kind)
            v['events'].pop(index)
            for i,e in enumerate(v['events']):e['sequence']=i
            self.reject(v)
    def test_raw_ack_is_not_validated_ack(self):
        v=self.good();next(e for e in v['events'] if e['kind']=='completion-ack')['value']['validated']=False;self.reject(v)
        v=self.good();next(e for e in v['events'] if e['kind']=='completion-ack')['value']['model']='old';self.reject(v)
    def test_duplicate_choice_or_ack_fails(self):
        for kind in ('completion-choose','completion-ack','picker-ack'):
            v=self.good();e=copy.deepcopy(next(e for e in v['events'] if e['kind']==kind));e['sequence']=len(v['events']);v['events'].append(e);self.reject(v)
    def test_ticket_catalog_owner_sequence_and_snapshot_are_bound(self):
        for field in ('owner','catalog','seq','choice','snapshot'):
            v=self.good();t=next(e for e in v['events'] if e['kind']=='completion-choose')['value']['ticket']
            t[field]=dict(t[field],direction='backward') if field=='snapshot' else 0 if field=='seq' else 'other';self.reject(v)
    def test_unicode_selection_and_wire_text_cannot_drift(self):
        for field,value in [('text','wrong'),('start',1),('utf16Length',1),('codePoints',[1]),('direction','backward')]:
            v=self.good();v['actions'][0]['after'][field]=value;self.reject(v)
    def test_picker_cancellation_preserves_backward_selection_and_focus(self):
        for field,value in [('start',4),('direction','forward'),('focused',False)]:
            v=self.good();v['actions'][2]['after'][field]=value;self.reject(v)
    def test_unready_choice_stays_disabled_then_valid_choice_is_unique(self):
        v=self.good();v['actions'][3]['blocked']['control']['disabled']=False;self.reject(v)
        v=self.good();next(g for g in v['geometry'] if g['stage']=='picker-unready')['pickerReady']=True;self.reject(v)
        v=self.good();next(e for e in v['events'] if e['kind']=='picker-ack')['value']['confirmed']=False;self.reject(v)
    def test_readiness_cannot_replace_or_revive_native_picker(self):
        for mutation in ('replacement','revival','serial'):
            v=self.good()
            if mutation=='serial':next(e for e in v['events'] if e['kind']=='picker-choose')['value']['serial']=1
            else:next(g for g in v['geometry'] if g['stage']=='picker-ready')['pickerNode']='3' if mutation=='replacement' else '1'
            self.reject(v)
    def test_newer_owner_catalog_mount_navigation_fences_are_real(self):
        for stage,key in [('guard-newer','text'),('guard-catalog','catalog'),('guard-account','epoch'),('guard-mount','view'),('guard-navigation','channelGeneration')]:
            v=self.good();a=next(a for a in v['actions'] if a['kind']==stage)
            if key=='text':a['after']['text']=R['QUERY']
            else:a['ownerAfter'][key]=a['ownerBefore'][key]
            self.reject(v)
    def test_composition_event_cannot_be_invented_or_trusted(self):
        for field,value in [('type','input'),('bubbles',False),('isTrusted',True)]:
            v=self.good();v['actions'][-1]['composition'][field]=value;self.reject(v)
    def test_geometry_requires_nine_owned_points_and_full_containment(self):
        for mutation in ('count','occluded','coordinate','offscreen','parent'):
            v=self.good();row=v['geometry'][0];p=row['controls']['field']
            if mutation=='count':p['points'].pop()
            elif mutation=='occluded':p['points'][4]['owned']=False
            elif mutation=='coordinate':p['points'][0]['x']+=1
            elif mutation=='offscreen':p.update(proof(bounds=dict(x=-5,y=10,width=120,height=30)),visible=False)
            else:row['controls']['footer']=proof(bounds=dict(x=20,y=10,width=120,height=30))
            self.reject(v)
    def test_artifacts_and_closed_log_cannot_escape_or_change(self):
        v=self.good();edges=[dict(stage=s,edge=e) for s in R['STAGES'] for e in ('before','after')]
        self.assertEqual(len(R['validate_native'](v['fixture'],self.nonce,self.pid,self.dir,v,edges)),16)
        old=v['captures']['initial'];v['captures']['initial']=str(self.dir/'..'/'initial.png')
        with self.assertRaises(RuntimeError):R['validate_native'](v['fixture'],self.nonce,self.pid,self.dir,v,edges)
        v['captures']['initial']=old;ready=R['ready_tuple'](v['fixture'],self.nonce,self.pid)
        lines=[R['WAITING']+json.dumps(ready)]+[R['STAGE']+json.dumps(e) for e in edges]+[R['PREFIX']+json.dumps(v)]
        path=self.dir/'native.log';path.write_text('\n'.join(lines)+'\n')
        R['validate_closed_log'](path,ready,self.dir,v)
        path.write_text('\n'.join(lines+[lines[-1]])+'\n')
        with self.assertRaises(RuntimeError):R['validate_closed_log'](path,ready,self.dir,v)
    def test_message_and_model_side_effects_rejected(self):
        v=self.good();v['messagesFinal']['channels']['C2']['pending'].append('new');self.reject(v)
        v=self.good();v['geometry'][0]['model']['text']='other';self.reject(v)
    def test_inserted_direction_is_explicit_forward(self):
        for action in (0,1,3):
            for direction in ('none','backward'):
                v=self.good();v['actions'][action]['after']['direction']=direction;self.reject(v)
    def test_same_mount_guards_preserve_selection_and_focus(self):
        for stage in ('guard-newer','guard-catalog','guard-composition'):
            for field,value in (('start',0),('direction','backward'),('focused',False)):
                v=self.good();next(a for a in v['actions'] if a['kind']==stage)['after'][field]=value;self.reject(v)
    def test_completed_controller_decisions_cannot_be_missing_or_fabricated(self):
        for mutation in ('missing','wrong-owner','wrong-result','accepted','not-composing','missing-begin','missing-rust-denial'):
            v=self.good();ledger=v['controllerEvents']
            if mutation=='missing':ledger.pop(2)
            elif mutation=='wrong-owner':ledger[2]['owner']='wrong'
            elif mutation=='wrong-result':ledger[2]['result']['serial']+=1
            elif mutation=='accepted':ledger[2]['accepted']=True
            elif mutation=='not-composing':ledger[5]['composing']=False
            elif mutation=='missing-begin':ledger.pop(4)
            else:
                v['events']=[e for e in v['events'] if not(e['stage']=='guard-catalog' and e['kind']=='completion-admission')]
                for i,e in enumerate(v['events']):e['sequence']=i
            self.reject(v)

if __name__=='__main__':unittest.main()
