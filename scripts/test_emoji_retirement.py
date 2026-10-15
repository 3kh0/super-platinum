import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import runpy
import emoji_retirement_receipts as R
from emoji_retirement_windows import Capabilities
import emoji_retirement_overhead as O

IDENTITY=dict(pid=47,nonce='1234567890abcdef'*2,process='W1')

def event(kind,at,gen=5,child=0,context=0,write=0,related=0,thread=11,action=0):
    return [0,at,kind,gen,child,context,thread,1 if thread==11 else 2,write,0,action,related,0,0,0,0]

def blob(rows):
    data=bytearray(R.BYTES)
    struct.pack_into('<12Q',data,0,R.MAGIC,1,R.CAPACITY,len(rows),0,11,0,0,47,int(IDENTITY['nonce'][:16],16),int(IDENTITY['nonce'][16:],16),1)
    for i,row in enumerate(rows):
        row=list(row);row[0]=i+1;struct.pack_into('<16Q',data,(12+i*16)*8,*row)
    return data

def chain():
    return [event(1,1,child=1,thread=12),event(20,2,gen=0,context=1,write=2,related=1)]+[
        event(k,k+1,child=1,context=1,write=2) for k in (2,3,4,5)]+[
        event(21,7,gen=0,context=1,write=2,related=0)]

class Trace(unittest.TestCase):
    def test_actual_payload_chain_and_unknown_survive(self):
        trace=R.decode(blob(chain()),IDENTITY)
        self.assertEqual(len(trace['events']),7)
        self.assertEqual(trace['events'][2]['context'],1)
        # Unknown destruction is structurally valid, classified later as censored.
        R.decode(blob([event(k,k,child=1,thread=12 if k==1 else 11) for k in (1,2,3,4,5)]),IDENTITY)

    def test_each_header_identity_is_bound(self):
        for index in (0,1,2,4,5,7,8,9,10,11):
            with self.subTest(index=index):
                b=blob(chain());v=struct.unpack_from('<Q',b,index*8)[0];struct.pack_into('<Q',b,index*8,v+1)
                with self.assertRaises(ValueError):R.decode(b,IDENTITY)

    def test_overflow_and_unpublished_slot_refused(self):
        for offset,value in ((3*8,R.CAPACITY+1),(12*8,0),((12+6*16)*8,4)):
            b=blob(chain());struct.pack_into('<Q',b,offset,value)
            with self.assertRaises(ValueError):R.decode(b,IDENTITY)

    def test_duplicate_payload_and_missing_phase_refused(self):
        for rows in (chain()[:1]+chain(),chain()[:3]+chain()[4:]):
            with self.assertRaises(ValueError):R.decode(blob(rows),IDENTITY)

    def test_context_guard_and_thread_fabrication_refused(self):
        for offset,value in ((5,2),(8,1),(7,2),(6,99)):
            rows=chain();rows[3][offset]=value
            with self.assertRaises(ValueError):R.decode(blob(rows),IDENTITY)

    def test_unknown_context_never_becomes_known_attribution(self):
        rows=chain();rows[3][5]=0
        with self.assertRaises(ValueError):R.decode(blob(rows),IDENTITY)

    def test_failure_and_incomplete_tail_not_hidden(self):
        with self.assertRaises(ValueError):R.decode(blob([event(17,1,related=21)]),IDENTITY)
        b=blob(chain());b[-1]=1
        with self.assertRaises(ValueError):R.decode(b,IDENTITY)

    def test_threads_need_not_share_slot_order_clock(self):
        # Reservation order across threads is not temporal order.
        R.decode(blob([event(6,20,thread=11),event(6,10,thread=12)]),IDENTITY)

class FakeAPI:
    def __init__(self):self.calls=[];self.number=100;self.ended=0;self.fail=set()
    def call(self,name,*args):
        self.calls.append((name,*args))
        if name in self.fail:raise OSError(name)
        self.number+=1;return self.number
    def event(self):return self.call('event')
    def mapping(self):return self.call('mapping')
    def view(self,h):return self.call('view',h)
    def initialize(self,v,h):self.call('initialize',v,h)
    def open(self,pid):return self.call('open',pid)
    def times(self,h):self.call('times',h);return {'creation':23,'exit':self.ended}
    def duplicate(self,h,p,r):return self.call('duplicate',h,p,r)
    def reclaim(self,p,h):self.call('reclaim',p,h)
    def close(self,h):self.call('close',h)
    def unmap(self,h):self.call('unmap',h)
    def signal(self,h):self.call('signal',h)
    def window_pid(self,h):self.call('window_pid',h);return 47
    def post_close(self,h):self.call('post_close',h)
    def read(self,v):return self.call('read',v)

class Clock:
    def __init__(self):self.now=0
    def __call__(self):return self.now
    def sleep(self,s):self.now+=round(s*1e9)

class Handles(unittest.TestCase):
    def attached(self,role='B1'):
        api=FakeAPI();cap=Capabilities(role,api);ready=cap.attach(47,23,IDENTITY['nonce']);return api,cap,ready

    def test_event_before_process_and_minimum_duplicate_rights(self):
        api,cap,ready=self.attached();self.assertEqual(api.calls[0][0],'event')
        self.assertEqual([c[-1] for c in api.calls if c[0]=='duplicate'],[6,0x100000])
        self.assertIsInstance(ready['event'],str);cap.close()

    def test_unpublished_duplicates_reclaimed_exactly_once(self):
        api,cap,_=self.attached();cap.close();cap.close()
        self.assertEqual(len([x for x in api.calls if x[0]=='reclaim']),2)

    def test_published_lost_ack_never_reclaims_numeric_handle(self):
        api,cap,ready=self.attached()
        with tempfile.TemporaryDirectory() as tmp:cap.publish(Path(tmp),ready)
        cap.close();self.assertFalse(any(x[0]=='reclaim' for x in api.calls))
        self.assertTrue(any(x[0]=='signal' for x in api.calls))

    def test_uncertain_publication_is_not_reclaimed(self):
        api,cap,ready=self.attached()
        with tempfile.TemporaryDirectory() as tmp,patch('emoji_retirement_windows.os.replace',side_effect=OSError('uncertain')):
            with self.assertRaises(OSError):cap.publish(Path(tmp),ready)
        cap.close();self.assertFalse(any(x[0]=='reclaim' for x in api.calls))

    def test_checked_post_precedes_exact_independent_signal(self):
        api,cap,_=self.attached();clock=Clock();value=cap.close_request(9,lambda:None,clock,clock.sleep)
        self.assertEqual(value['signal_ns']-value['request_ns'],100_000_000)
        kinds=[x[0] for x in api.calls];self.assertLess(kinds.index('post_close'),kinds.index('signal'));cap.close()

    def test_failed_post_releases_without_retry(self):
        api,cap,_=self.attached();api.fail.add('post_close');clock=Clock()
        with self.assertRaises(OSError):cap.close_request(9,lambda:None,clock,clock.sleep)
        self.assertEqual(sum(x[0]=='post_close' for x in api.calls),1)
        self.assertTrue(any(x[0]=='signal' for x in api.calls));cap.close()

    def test_early_exit_and_late_signal_remain_failures(self):
        for late in (False,True):
            api,cap,_=self.attached();clock=Clock()
            def sleep(s):clock.now+=300_000_000 if late else round(s*1e9)
            with self.assertRaises(ValueError):cap.close_request(9,lambda:None if late else 0,clock,sleep)
            cap.close()

    def test_cleanup_continues_through_all_close_failures(self):
        api,cap,_=self.attached();api.fail.update(('signal','unmap','close','reclaim'));cap.close()
        self.assertGreaterEqual(len(cap.errors),7)
        self.assertIsNone(cap.process);self.assertIsNone(cap.view)

    def test_no_trace_read_while_child_alive(self):
        api,cap,_=self.attached()
        with self.assertRaises(ValueError):cap.snapshot_after_exit()
        api.ended=1;cap.snapshot_after_exit();cap.published=True;cap.close()

class RunnerFailures(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module=runpy.run_path(str(Path(__file__).with_name('check-emoji-retirement-native.py')))

    def test_event_creation_and_final_hash_failure_retain_primary_receipt(self):
        one=self.module['one'];scope=one.__globals__
        class Native:
            def __init__(self,fixture,log,*args):self.log=log;self.process=None;self.cleanup_errors=[]
        def failed_event(*args):raise OSError('event-create-primary')
        def failed_hash(*args):raise FileNotFoundError('final-hash-secondary')
        with tempfile.TemporaryDirectory() as tmp,patch.dict(scope,NativeFixture=Native,Capabilities=failed_event,digest=failed_hash):
            folder=Path(tmp)/'B1'
            with self.assertRaisesRegex(OSError,'event-create-primary'):
                one('B1',Path(tmp)/'missing.exe',folder,{'binary_sha256':'x'})
            result=json.loads((folder/'receipt.json').read_text())
            self.assertFalse(result['ok']);self.assertIn('event-create-primary',result['error'])
            self.assertIn('final-hash-secondary',result['binaryHashError'])

    def test_false_shutdown_stops_before_a_second_process(self):
        main=self.module['main'];scope=main.__globals__;calls=[]
        with tempfile.TemporaryDirectory() as tmp:
            args=type('Args',(),dict(binary=Path(tmp)/'bin',build_receipt=Path(tmp)/'build',overhead_gate=Path(tmp)/'gate',output=Path(tmp)/'out'))()
            def one(role,*args):calls.append(role);return {'process':role,'ok':False}
            with patch('argparse.ArgumentParser.parse_args',return_value=args),patch.object(scope['sys'],'platform','win32'),patch.dict(scope,
                sealed=lambda *args:{'manifest_sha256':'a'},overhead_gate=lambda *args:{'ok':True},one=one):
                with self.assertRaisesRegex(ValueError,'Stop on failed'):main()
            self.assertEqual(calls,['W1']);report=json.loads((args.output/'report.json').read_text())
            self.assertFalse(report['ok']);self.assertEqual(len(report['processes']),1)

    def test_changed_provenance_before_next_process_is_retained(self):
        main=self.module['main'];scope=main.__globals__;calls=[];checks=[]
        with tempfile.TemporaryDirectory() as tmp:
            args=type('Args',(),dict(binary=Path(tmp)/'bin',build_receipt=Path(tmp)/'build',overhead_gate=Path(tmp)/'gate',output=Path(tmp)/'out'))()
            def sealed(*args):
                checks.append(1)
                if len(checks)>=3:raise ValueError('changed-source')
                return {'manifest_sha256':'a'}
            def one(role,*args):calls.append(role);return {'process':role,'ok':True}
            with patch('argparse.ArgumentParser.parse_args',return_value=args),patch.object(scope['sys'],'platform','win32'),patch.dict(scope,
                sealed=sealed,overhead_gate=lambda *args:{'ok':True},one=one):
                with self.assertRaisesRegex(ValueError,'changed-source'):main()
            self.assertEqual(calls,['W1']);report=json.loads((args.output/'report.json').read_text())
            self.assertFalse(report['ok']);self.assertIn('changed-source',report['terminalProvenanceError'])

def query_case():
    f=dict(scope='scope',mount='mount',wire=[':ro',3,3,'forward'],catalog='1/local',preferences='1',ready='true',connected=True)
    choices=[dict(label=':rocket:',detail='rocket')]
    q=dict(field='channel-composer',seq=1,owner='scope:mount:1',snapshot=json.dumps(f['wire'],separators=(',',':')))
    result=dict(kind='emoji_results',seq=1,owner=q['owner'],catalog='1/local',preferences='1',choices=choices)
    a=dict(field=q['field'],owner=q['owner'],snapshot=q['snapshot'],result=result,valid=True,at=2)
    op=dict(id='W1/c1/completion.channel/query',kind='ordinary',subcase='completion.channel')
    witness=dict(focused=True,visibility='visible',active='channel-composer',channel=f,thread=None,popup=dict(connected=True,hidden=False,rows=choices),picker=None)
    action=dict(id=op['id'],acceptedAt=3,accepted=dict(owner=q['owner'],seq=1,catalog='1/local',at=2),
        before=dict(channel={**f,'wire':['',0,0,'forward']},thread=None),after=dict(channel=f,thread=None),
        witnesses=[copy.deepcopy(witness) for _ in range(3)],ack=dict(ok=True,id=op['id'],nativeComplete=True,generation=0,state=dict(isolated=True,team='T1',account='emoji-retirement-0',epoch=0,catalog=dict(revision=3,ready=dict(token='1/local',generation=5,revision=3)),prefs=dict(loaded=True,revision=1))))
    events=[dict(kind='query',value=q,at=1,action=op['id']),dict(kind='accepted',value=a,at=2,action=op['id'])]
    return action,op,events

class Authority(unittest.TestCase):
    def test_full_snapshot_authority_and_three_witnesses(self):R.validate_authority(*query_case(),0,3)
    def test_each_owner_and_dom_corruption_refused(self):
        paths=[('ack','state','epoch'),('ack','state','catalog','revision'),('ack','state','prefs','loaded'),
               ('accepted','catalog'),('accepted','seq'),('witnesses',1,'channel','scope'),('witnesses',1,'channel','mount'),
               ('witnesses',1,'channel','preferences'),('witnesses',1,'channel','wire',3),
               ('witnesses',2,'popup','rows',0,'label'),('witnesses',2,'active'),('witnesses',0,'focused')]
        for path in paths:
            with self.subTest(path=path):
                action,op,events=query_case();obj=action
                for key in path[:-1]:obj=obj[key]
                obj[path[-1]]='corrupt'
                with self.assertRaises(ValueError):R.validate_authority(action,op,events,0,3)
    def test_actual_query_and_acceptance_cannot_be_invented(self):
        for index,key in ((0,'snapshot'),(0,'seq'),(1,'owner'),(1,'valid')):
            action,op,events=query_case();events[index]['value'][key]='corrupt'
            with self.assertRaises(ValueError):R.validate_authority(action,op,events,0,3)
    def test_bool_and_float_are_not_integer_identities(self):
        for path,value in [(('ack','state','epoch'),False),(('ack','state','catalog','revision'),3.0),
                           (('ack','generation'),False),(('accepted','seq'),True)]:
            action,op,events=query_case();obj=action
            for key in path[:-1]:obj=obj[key]
            obj[path[-1]]=value
            with self.assertRaises(ValueError):R.validate_authority(action,op,events,0,3)

def target_case():
    role='W1';actions=[];operations=[];events=[];targets={};subcases=[]
    def add(id,generation,held=0):
        token=f'{(generation&((1<<48)-1))>>2}/local'
        a={'id':id,'ack':{'generation':held,'state':{'catalog':{'ready':{'generation':generation,'token':token}}}},'accepted':{'catalog':token}}
        actions.append(a);operations.append(dict(id=id,kind='ordinary'));return a
    add('seed',5)
    for sub,verb,g,current in [('refresh.closed','refresh',5,9),('refresh.picker','open',9,9),
        ('completion.channel','query',13,13),('completion.thread','query',13,13),
        ('reject.local','start',17,13),('reject.remote','start',26,25),('account.reset','open',29,29)]:
        key=role+'/c1/'+sub;subcases.append(dict(id=key,subcase=sub));targets[key]=g
        add(key+'/'+verb,current,g if sub.startswith('reject.') else 0)
        for child in (1,2):events.append(dict(kind=9,related=R.fnv(key),generation=g,child=child,action=len(actions)))
        if sub.startswith('reject.'):add(key+'/release',current,g)
    g=(3<<48)|5;targets[role+'/shutdown']=g;add(role+'/setup/shutdown/query',g)
    process=dict(process=role,cycles=[dict(account_ordinal=0,subcases=subcases)])
    return process,dict(actions=actions,targets=targets),events,operations

class Targets(unittest.TestCase):
    def test_actual_preaction_and_accepted_generation_binding(self):R.validate_target_authority(*target_case())
    def test_self_consistent_terminal_and_ledger_substitution_refused(self):
        for sub in ('refresh.closed','refresh.picker','completion.channel','completion.thread','reject.local','reject.remote','account.reset'):
            p,t,e,ops=target_case();key='W1/c1/'+sub;t['targets'][key]+=4
            for event in e:
                if event['related']==R.fnv(key):event['generation']=t['targets'][key]
            with self.assertRaises(ValueError):R.validate_target_authority(p,t,e,ops)
    def test_right_target_at_wrong_operation_refused(self):
        p,t,e,ops=target_case();e[0]['action']+=1
        with self.assertRaises(ValueError):R.validate_target_authority(p,t,e,ops)

class Overhead(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        rows=[];blocks=[]
        for block in range(100):
            on=block%2==1;start=block*1_000_000+10000;end=start+10000
            blocks.append(dict(block=block,enabled=on,drops=100,nanoseconds=1000 if not on else 2000,record_start_ns=start,record_end_ns=end))
            if on:
                for ticket in range((block//2)*100+1,(block//2)*100+101):
                    for k in (10,1,11):rows.append(event(k,start-100,gen=(ticket<<2)|1,child=1 if k==1 else 0))
                for ticket in range((block//2)*100+1,(block//2)*100+101):
                    for k in (2,3,4,5):rows.append(event(k,start+100,gen=(ticket<<2)|1,child=1))
        cls.on=blob(rows);cls.off=blob([])
        # Overhead ledgers use a dedicated zero nonce, never native role evidence.
        for b in (cls.on,cls.off):struct.pack_into('<2Q',b,9*8,0,0)
        cls.receipt=dict(schema=1,kind='empty-wrapper-overhead',manifest_sha256='a'*64,pid=47,nonce='0'*32,process='W1',
            blocks=blocks,disabled_median_ns_per_drop=10.,enabled_median_ns_per_drop=20.,difference_ns_per_drop=10.,threshold_ns_per_drop=10000,
            enabled_child_events=25000,enabled_total_events=35000,disabled_total_events=0,complete=True,ok=True)
    def test_exact_count_arithmetic_and_actual_phase_ledger(self):O.validate(self.receipt,self.off,self.on,'a'*64)
    def test_mutated_count_arm_clock_and_threshold_refused(self):
        for path,value in [(('blocks',1,'enabled'),False),(('blocks',0,'drops'),True),
            (('blocks',1,'record_start_ns'),0),(('difference_ns_per_drop',),0),(('threshold_ns_per_drop',),10001)]:
            r=copy.deepcopy(self.receipt);obj=r
            for key in path[:-1]:obj=obj[key]
            obj[path[-1]]=value
            with self.assertRaises(ValueError):O.validate(r,self.off,self.on,'a'*64)
    def test_overhead_cannot_be_claimed_as_native_producer_trace(self):
        with self.assertRaises(ValueError):R.decode(self.on,dict(pid=47,nonce='0'*32,process='W1'))

class Statistics(unittest.TestCase):
    def cohort(self):
        result=[]
        for role in R.ROLES:
            rows=[]
            for cycle in (1,2,3):
                for sub in ('refresh.closed','refresh.picker','completion.channel','completion.thread','reject.local','reject.remote','account.reset'):
                    for child in (1,2):rows.append(dict(process=role,cycle=cycle,subcase=sub,classification='fault' if sub.startswith('reject') else 'ordinary',child=child,context=1,status='completed',serial_ns=6_000_000,payload_ns=5_500_000,role=1))
            result.append(dict(process=role,analysis=dict(rows=rows,shutdown=dict(complete=True))))
        return result
    def test_process_first_and_fault_exclusion(self):
        result=R.aggregate(self.cohort());self.assertTrue(result['ordinary_screen'])
        self.assertFalse(any(g['subcase'].startswith('reject') for g in result['groups']))
    def test_missing_cycle_and_duplicate_process_refused(self):
        values=self.cohort();values[0]['analysis']['rows'].pop()
        with self.assertRaises(ValueError):R.aggregate(values)
        values=self.cohort();values[1]['process']='W1'
        with self.assertRaises(ValueError):R.aggregate(values)
    def test_late_unknown_missing_never_zero(self):
        for status in ('outside-cutoff','unknown-context','missing-terminal'):
            values=self.cohort()
            for r in values:
                for x in r['analysis']['rows']:x['status']=status
            self.assertFalse(R.aggregate(values)['ordinary_screen'])
    def test_clean_exit_is_not_drain(self):
        values=self.cohort();values[0]['analysis']['shutdown']['complete']=False
        self.assertFalse(R.aggregate(values)['shutdown_complete'])

class Censoring(unittest.TestCase):
    def sample(self,final_at=6_000_000_000,omit=None):
        # One real generation bound to two different consumer windows: a later
        # destructor may qualify the later window only, never be backdated.
        cases=[dict(id='W1/c1/'+s,subcase=s,classification='ordinary',actions=['a'+str(i)]) for i,s in enumerate(('completion.channel','completion.thread'))]
        p=dict(process='W1',cycles=[dict(cycle=1,subcases=cases)],ordered_actions=[dict(id='a0',kind='ordinary'),dict(id='a1',kind='ordinary')])
        terminal=dict(targets={s['id']:5 for s in cases})
        events=[]
        for child in (1,2):
            for kind,at in ((1,1),(2,final_at-3),(3,final_at-2),(4,final_at-1),(5,final_at)):
                if (child,kind)==omit:continue
                events.append(dict(kind=kind,at=at,generation=5,child=child,context=1,thread=11,role=1,write=2))
        events.append(dict(kind=22,at=7_000_000_000))
        with patch.object(R,'validate_actions',return_value=(p,[dict(at=100),dict(at=2_000_000_000)])):
            return R.samples({},terminal,{'events':events})

    def test_late_shared_generation_not_backdated_or_duplicated(self):
        value=self.sample();self.assertEqual([r['status'] for r in value['rows']],['outside-cutoff']*2+['completed']*2)
        self.assertEqual(value['shutdown']['created'],[[5,1],[5,2]])
        self.assertTrue(value['shutdown']['complete'])

    def test_missing_child_terminal_censors_and_fails_shutdown(self):
        value=self.sample(omit=(2,5));self.assertFalse(value['shutdown']['complete'])
        self.assertEqual(value['shutdown']['missing'],[[5,2]])
        self.assertEqual(value['rows'][3]['status'],'missing-terminal')

    def test_terminal_after_shutdown_cutoff_is_not_drain(self):
        value=self.sample(13_000_000_000);self.assertFalse(value['shutdown']['complete'])
        self.assertEqual(len(value['shutdown']['missing']),2)

class Schedule(unittest.TestCase):
    def test_exact_frozen_schedule_inventory(self):
        schedule=json.loads(Path(__file__).with_name('emoji_retirement_schedule.json').read_text())
        self.assertEqual([p['process'] for p in schedule['processes']],list(R.ROLES))
        actions=[x for p in schedule['processes'] for x in p['ordered_actions'] if x['kind']!='cutoff']
        self.assertEqual(len(actions),525);self.assertEqual(len({x['id'] for x in actions}),525)
        self.assertEqual(sum(x['kind']=='setup' for x in actions),177)
        self.assertEqual(sum(x['kind']=='ordinary' for x in actions),234)
        self.assertEqual(sum(x['kind']=='fault' for x in actions),108)
        self.assertEqual(sum(x['kind']=='shutdown' for x in actions),6)
        self.assertEqual(sum(x['kind']=='cutoff' for p in schedule['processes'] for x in p['ordered_actions']),126)

if __name__=='__main__':unittest.main()
