"""Portable Profile clock/native receipt mutations; no app launches."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import profile_local_time as O


def sample(fixture='profile-pane', metadata_render_first=True):
    hover=fixture.startswith('profile-hover');viewport=O.expected_viewport(fixture)
    rect=dict(x=10,y=10,width=180,height=200)
    composer=dict(nodeId=3,connected=True,rect=dict(x=200,y=280,width=300,height=60),value=O.DRAFT,selectionStart=2,selectionEnd=9)
    ticks=[];writes=[];rows=[];previous=[];renders=[];displayed=[];pending=[]
    for i,name in enumerate(O.STAGES):
        closed=i==7;replacement=i==8
        if 1<=i<=4 or closed:writes.append(dict(now=O.EPOCHS[i]))
        m=dict(account='fixture-A' if not replacement else 'profile-clock-replacement',epoch=int(replacement),team='T1',user=None if closed else 'U2' if replacement else 'U1',offset=O.OFFSETS[i],now=O.EPOCHS[i],profileUser=None if closed or hover else 'U2' if replacement else 'U1',hoverUser=('U2' if replacement else 'U1') if hover and not closed else None,composer=dict(text=O.DRAFT,selection=dict(start=0,end=0)),threadComposer=dict(text=''),drafts=dict(channel=None,thread=None,items=[]),mainAttachments=[],threadAttachments=[],channel='C2',channelGeneration=1,profileGeneration=int(replacement),hoverGeneration=int(replacement))
        after_tick=len(ticks);current=O.face(m,hover)
        # Initial/metadata/replacement rendering precedes poll; pure clock boundaries
        # require the runtime notification before the new rendered label exists.
        if i in (0,8) or i in (5,6) and metadata_render_first:
            renders.append(dict(serial=len(renders)+1,now=m['now'],face=copy.deepcopy(current[0])))
            displayed=copy.deepcopy(current)
        for poll in range(2):
            displayed=[old for old in displayed if any(O.same_owner(old,new) for new in current)]
            changed=[new for new in current if any(O.same_owner(old,new) and old['minute']!=new['minute'] for old in displayed)]
            notify=any(new not in pending for new in changed);pending=copy.deepcopy(changed)
            ticks.append(dict(serial=len(ticks)+1,renderSerial=len(renders),now=m['now'],before=copy.deepcopy(displayed),after=copy.deepcopy(current),notify=notify))
            if notify:
                renders.append(dict(serial=len(renders)+1,now=m['now'],face=copy.deepcopy(current[0])))
                displayed=copy.deepcopy(current)
        n=dict(ok=True,viewport=viewport,root=None if closed else dict(nodeId=4 if replacement else 1,connected=True,rect=rect),time=None if i in (6,7) else dict(nodeId=5 if replacement else 2,connected=True,rect=dict(x=20,y=100,width=140,height=20),text=O.label(m['now'],m['offset'])),original=dict(rootId=1,rootConnected=i<7,timeId=2,timeConnected=i<6,timeText=O.label(O.EPOCHS[min(i,5)],O.OFFSETS[min(i,5)])),scroll=None if hover or closed else dict(top=0,left=0),composer=copy.deepcopy(composer),documentWidth=viewport[0])
        if replacement:n['composer'].update(nodeId=6,selectionStart=0,selectionEnd=0)
        if i==0:action=dict(ok=True,kind='synthetic-dom-input',nodeId=3,rect=copy.deepcopy(composer['rect']),setup=None,hitPoint=dict(x=220,y=300),isTrusted=False,value=O.DRAFT,selectionStart=2,selectionEnd=9)
        elif i<=4:action=dict(kind='synthetic-clock',now=O.EPOCHS[i],afterTick=after_tick)
        elif i<=6:action=dict(kind='synthetic-member-update',user='U1',offset=O.OFFSETS[i])
        elif i==7:action=dict(ok=True,kind='synthetic-dom-mouseleave' if hover else 'synthetic-dom-click',isTrusted=False)
        else:action=dict(kind='synthetic-owner-replacement',user='U2')
        rows.append(dict(ok=True,stage=name,ordinal=i,noSession=True,noTransport=True,model=m,native=n,clock=dict(ticks=copy.deepcopy(ticks),renders=copy.deepcopy(renders),writes=copy.deepcopy(writes),notifications=sum(t['notify'] for t in ticks)),action=action))
    return dict(ok=True,noSession=True,noTransport=True,stagesCount=9,stages=rows,model=copy.deepcopy(rows[-1]['model']),clock=copy.deepcopy(rows[-1]['clock']),native=copy.deepcopy(rows[-1]['native']),layout=dict(ok=True,viewport=viewport))


def setup_sample():
    data=sample('profile-pane-narrow');initial=data['stages'][0];action=initial['action']
    action['rect']=dict(x=200,y=280,width=300,height=60);action['hitPoint']=dict(x=220,y=300)
    # Reopened overlay geometry is deliberately different from the input-time rect.
    for row in data['stages']:row['native']['composer']['rect']=dict(x=380,y=280,width=200,height=60)
    data['native']=copy.deepcopy(data['stages'][-1]['native'])
    model=copy.deepcopy(initial['model']);model.update(user=None,offset=None,profileUser=None,hoverUser=None)
    native=copy.deepcopy(initial['native']['composer']);native['rect']=copy.deepcopy(action['rect'])
    def click(node,label):
        return dict(ok=True,kind='synthetic-dom-click',nodeId=node,rect=dict(x=10,y=10,width=80,height=20),hitPoint=dict(x=20,y=20),isTrusted=False,label=label)
    action['setup']=dict(close=click(10,'Close profile'),draft=dict(model=model,native=native),
        reopen=dict(**click(11,'Maya Chen'),messageId='m-rich-1'),selection=dict(ok=True,kind='native-selection-restore',nodeId=3,before=dict(start=0,end=0),after=dict(start=2,end=9)))
    return data


def wrapper():
    spec=importlib.util.spec_from_file_location('profile_time_wrapper',Path(__file__).with_name('check-profile-local-time-native.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module


def change(data,path,value):
    for key in path[:-1]:data=data[key]
    data[path[-1]]=value


class ProfileTime(unittest.TestCase):
    def reject(self,path,value,fixture='profile-pane'):
        data=sample(fixture);change(data,path,value)
        with self.assertRaises(RuntimeError):O.validate(data,fixture)

    def test_six_original_shapes_and_independent_midnight(self):
        for fixture in O.FIXTURES:
            with self.subTest(fixture=fixture):O.validate(sample(fixture),fixture)
        self.assertEqual([O.label(O.EPOCHS[i],O.OFFSETS[i]) for i in range(9)],['23:58 local time','23:58 local time','23:59 local time','23:59 local time','00:00 local time','12:30 local time',None,None,'08:01 local time'])
        self.assertEqual(O.label(29,31),'00:01 local time')
        self.assertEqual(O.label(0,-60),'23:59 local time')

    def test_metadata_poll_before_or_after_actual_render(self):
        for first in (True,False):
            receipt=sample(metadata_render_first=first)
            O.validate(receipt,'profile-pane')
            self.assertEqual(receipt['clock']['notifications'],2 if first else 4)
        # The actual render journal, not a permissive count range, admits the race.
        bad=sample(metadata_render_first=False)
        bad['stages'][5]['clock']['renders'][-1]['face']['minute']=0
        with self.assertRaises(RuntimeError):O.validate(bad,'profile-pane')

    def test_exposed_close_input_author_reopen_setup_only(self):
        O.validate(setup_sample(),'profile-pane-narrow')
        for path,value in [
            (('action','rect'),dict(x=0,y=0,width=5,height=5)),
            (('action','setup','close','hitPoint'),dict(x=-1,y=10)),
            (('action','setup','reopen','messageId'),'foreign'),
            (('action','setup','reopen','label'),'Other person'),
            (('action','setup','draft','model','profileUser'),'U1'),
            (('action','setup','draft','model','composer','text'),'lost'),
            (('action','setup','draft','native','nodeId'),99),
            (('action','setup','selection','after'),dict(start=0,end=0)),
        ]:
            bad=setup_sample();change(bad['stages'][0],path,value)
            with self.subTest(path=path),self.assertRaises(RuntimeError):O.validate(bad,'profile-pane-narrow')
        bad=setup_sample();bad['stages'][2]['native']['composer']['selectionStart']=0
        with self.assertRaises(RuntimeError):O.validate(bad,'profile-pane-narrow')
        # Setup selection restoration never permits later idle caret changes.
        bad=setup_sample();bad['stages'][2]['native']['composer']['nodeId']=99
        with self.assertRaises(RuntimeError):O.validate(bad,'profile-pane-narrow')

    def test_reopened_initial_render_defers_polls_only_before_clock_advance(self):
        data=setup_sample()
        data['stages'][0]['clock']['ticks']=data['stages'][0]['clock']['ticks'][:1]
        O.validate(data,'profile-pane-narrow')
        for path,value in [
            (('stages',1,'action','afterTick'),1),
            (('stages',1,'action','afterTick'),3),
            (('stages',1,'clock','ticks',1,'after',0,'generation'),99),
            (('stages',1,'clock','ticks',1,'before'),[]),
            (('stages',1,'clock','ticks',1,'now'),O.EPOCHS[1]),
            (('stages',0,'clock','renders',0,'face','generation'),99),
            (('stages',0,'action','setup'),None),
        ]:
            bad=copy.deepcopy(data);change(bad,path,value)
            with self.subTest(path=path),self.assertRaises(RuntimeError):O.validate(bad,'profile-pane-narrow')

    def test_actual_clock_only_transition_and_notification_count(self):
        self.reject(('stages',2,'action','kind'),'synthetic-render')
        self.reject(('stages',2,'action','afterTick'),999)
        self.reject(('stages',1,'clock','ticks',3,'notify'),True)
        self.reject(('stages',2,'clock','ticks',4,'notify'),False)
        self.reject(('stages',2,'clock','notifications'),0)
        self.reject(('stages',4,'native','time','text'),'23:00 local time')
        self.reject(('stages',4,'clock','writes',3,'now'),1735797600)

    def test_idle_node_scroll_and_caret_custody(self):
        self.reject(('stages',2,'native','root','nodeId'),99)
        self.reject(('stages',2,'native','time','nodeId'),99)
        self.reject(('stages',2,'native','original','timeConnected'),False)
        self.reject(('stages',2,'native','scroll','top'),30)
        self.reject(('stages',2,'native','composer','nodeId'),99)
        self.reject(('stages',2,'native','composer','selectionStart'),0)
        self.reject(('stages',2,'model','composer','text'),'consumed')
        self.reject(('stages',2,'model','drafts'),{'foreign':'draft'})
        self.reject(('stages',0,'action','hitPoint'),dict(x=0,y=0))

    def test_account_replacement_remount_preserves_data_not_caret(self):
        receipt=sample();O.validate(receipt,'profile-pane')
        self.assertEqual(receipt['stages'][7]['native']['composer']['nodeId'],3)
        self.assertEqual(receipt['stages'][8]['native']['composer']['nodeId'],6)
        self.reject(('stages',7,'native','composer','nodeId'),6)
        self.reject(('stages',7,'native','composer','selectionStart'),0)
        self.reject(('stages',8,'native','composer','nodeId'),3)
        self.reject(('stages',8,'native','composer','connected'),False)
        self.reject(('stages',8,'native','composer','value'),'lost')
        self.reject(('stages',8,'native','composer','selectionEnd'),999)
        self.reject(('stages',8,'model','composer','text'),'lost')
        # Removing the time row may clamp scroll; clock-only changes may not.
        receipt=sample();receipt['stages'][6]['native']['scroll']['top']=7
        O.validate(receipt,'profile-pane')
        self.reject(('stages',5,'native','scroll','top'),7)

    def test_offset_absence_and_retired_original_are_honest(self):
        self.reject(('stages',5,'model','offset'),0)
        self.reject(('stages',5,'action','kind'),'realtime-event')
        self.reject(('stages',6,'native','time'),sample()['stages'][5]['native']['time'])
        self.reject(('stages',7,'native','root'),sample()['stages'][0]['native']['root'])
        self.reject(('stages',7,'clock','ticks',14,'notify'),True)
        self.reject(('stages',8,'native','original','rootConnected'),True)
        self.reject(('stages',8,'native','original','timeText'),'08:01 local time')
        self.reject(('stages',8,'model','epoch'),0)
        self.reject(('stages',8,'native','root','nodeId'),1)
        self.reject(('stages',8,'clock','ticks',16,'notify'),True)

    def test_original_generation_and_owner_history(self):
        self.reject(('stages',2,'clock','ticks',4,'before',0,'generation'),999)
        self.reject(('stages',2,'clock','ticks',5,'after',0,'user'),'U2')
        self.reject(('stages',2,'model','hoverUser'),'U2')
        self.reject(('stages',8,'model','profileGeneration'),999)
        self.reject(('stages',8,'clock','ticks',16,'after',0,'account'),'fixture-A')
        self.reject(('stages',8,'action','kind'),'live-account-switch')

    def test_render_provenance_cannot_be_fabricated_or_borrowed(self):
        self.reject(('stages',2,'clock','ticks',4,'renderSerial'),999)
        self.reject(('stages',2,'clock','ticks',4,'before',0,'minute'),999)
        self.reject(('stages',2,'clock','renders',1,'face','generation'),99)
        self.reject(('stages',2,'clock','renders',1,'now'),1735809999)
        self.reject(('stages',2,'clock','renders'),[])
        self.reject(('stages',5,'clock','ticks',10,'notify'),True)

    def test_original_capture_terminal_geometry_and_isolation(self):
        self.reject(('stages',1,'ordinal'),2)
        self.reject(('stages',1,'noTransport'),False)
        self.reject(('stages',1,'native','documentWidth'),1400)
        self.reject(('stages',1,'native','time','rect','x'),-20)
        self.reject(('native','time','text'),'old')
        self.reject(('clock','ticks',17,'notify'),True)
        self.reject(('stagesCount',),8)

    def test_wrapper_closed_log_and_exact_hover_admission(self):
        w=wrapper()
        for fixture in O.FIXTURES:
            receipt=sample(fixture)
            with tempfile.TemporaryDirectory() as folder:
                base=Path(folder);w.validate(w.SPEC,fixture,receipt)
                log='\n'.join(O.PREFIX+'-stage: '+json.dumps(s) for s in receipt['stages'])+'\n'+O.PREFIX+'-check: '+json.dumps(receipt)+'\n'
                path=base/'native.log';path.write_text(log,encoding='utf8')
                stages=[{**s,'screenshot':str(i)+'.png'} for i,s in enumerate(receipt['stages'])]
                w.validate_closed(w.SPEC,fixture,path,stages,receipt)
                bad=copy.deepcopy(stages);bad[2]['native']['time']['text']='fake'
                with self.assertRaises(RuntimeError):w.validate_closed(w.SPEC,fixture,path,bad,receipt)
            native=SimpleNamespace(fixture=fixture,checks={w.SPEC.flag:'1'})
            with patch.object(w,'_normalize',return_value='pointer') as normalize:
                self.assertEqual(w.normalize(native,{'fixture':fixture},None,None,None),'pointer')
                self.assertEqual(normalize.call_args.kwargs['expected_profile_hover'],'U1' if fixture.startswith('profile-hover') else None)
            for wrong in (SimpleNamespace(fixture=fixture,checks={}),SimpleNamespace(fixture='foreign',checks={w.SPEC.flag:'1'})):
                with self.assertRaises(RuntimeError):w.normalize(wrong,{'fixture':fixture},None,None,None)
        with self.assertRaises(RuntimeError):w.validate(object(),O.FIXTURES[0],sample())

if __name__=='__main__':unittest.main()
