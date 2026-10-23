"""Focused startup producer/custody mutations. No renderer execution claim."""
import copy
import importlib.util
from pathlib import Path
import unittest
import workflow_management_share_startup as X
from test_workflow_management_share_empty import server


def sample(case='success',stage='share-open'):
    s=server('cold-offline');p=s['confirmationAdd']['share'];search=p['search'];search['case']='startup';search.pop('emptyCase')
    p['connectivity']['installed'].update(routable=True,transportHealth='unknown')
    pre=stage in X.A.PREFIX+('startup-before',);pending=pre and case in X.HELD
    methods=['client.gantryBoot','users.prefs.get']+(['experiments.getByUser'] if not pre and case=='success' else [])
    v=dict(case=case,requests=[],responses=[],settlements=[],releases=[],held=[2] if pending else [],networkCalls=0,storeInjection=False)
    for i,m in enumerate(methods,1):
        body=X.R.READ_BODIES[m] if m!='experiments.getByUser' else [['ssb_platform','windows'],['ssb_release_channel','prod']]
        v['requests'].append(dict(serial=i,method=m,body=body,endpoint='https://example.test/api/'+m,retrySafe=False,owner=copy.deepcopy(X.S.OWNER),rtGeneration=0,shareMounted=False))
        raw=X.R.observation_reply(m,'cold-mixed',s['seed']['boot']['self'])
        if i==2 and case=='missing':raw=dict(ok=True,opaque_receipt='original-startup-missing-prefs')
        v['responses'].append(dict(serial=i,error='offline') if i==2 and case=='failure' else dict(serial=i,raw=raw))
        if not(i==2 and pending):v['settlements'].append(dict(serial=i,returned=True))
    if not pre and case in X.HELD:v['releases']=[dict(serial=2,receiverRetired=False,delivered=True)]
    p['startup']=v
    obs=dict(currentComplete=not pre and case=='success',revision=2 if not pre and case=='prefs-retired' else 1,pending=pending,startupAttempted=not(not pre and case in ('prefs-retired','socket-retired')),reads=X.complete_reads(s) if not pre and case=='success' else None)
    return dict(proof=dict(server=s),model=dict(shareCustody=dict(startupObservations=obs)))


class StartupProducer(unittest.TestCase):
    def test_exact_original_pending_completed_and_refused(self):
        for case in X.CASES:
            a=sample(case,'startup-before');previous=X.producer(case,'startup-before',a)
            b=sample(case);previous=X.producer(case,'share-open',b,previous)
            X.producer(case,'offline-query',b,previous)
    def reject(self,mutate,case='success'):
        r=sample(case);mutate(r)
        with self.assertRaises(RuntimeError):X.producer(case,'share-open',r)
    def test_no_share_driven_first_acquisition(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup']['requests'][0].update(shareMounted=True))
    def test_wrong_original_account(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup']['requests'][0]['owner'].update(account='foreign'))
    def test_wrong_request_body(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup']['requests'][0].update(body=[]))
    def test_original_response_not_defaulted(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup']['responses'][1].update(raw=dict(ok=True,prefs={})))
    def test_missing_cannot_publish(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations'].update(currentComplete=True),'missing')
    def test_failure_cannot_publish(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations'].update(currentComplete=True),'failure')
    def test_prefs_late_original_cannot_publish(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations'].update(currentComplete=True),'prefs-retired')
    def test_socket_late_original_cannot_publish(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations'].update(currentComplete=True),'socket-retired')
    def test_missing_reservation_evidence_refused(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations'].update(startupAttempted=None),'socket-retired')
    def test_no_injected_store(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup'].update(storeInjection=True))
    def test_offline_share_cannot_read_preferences(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['search']['ranking']['requests'].append(dict(method='users.prefs.get')))
    def test_offline_share_cannot_search(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['search']['requests'].append(dict(method='users/search')))
    def test_offline_share_cannot_send(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['requests'].append(dict(method='chat.postMessage')))
    def test_complete_raw_receipts_retained(self):self.reject(lambda r:r['model']['shareCustody']['startupObservations']['reads'][0]['raw'].update(opaque_receipt='replaced'))
    def test_no_invented_receiver_cancellation(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['startup']['releases'][0].update(receiverRetired=True))
    def test_no_synthetic_http_success(self):self.reject(lambda r:r['proof']['server']['confirmationAdd']['share']['connectivity'].update(httpSuccessClaimed=True))
    def test_only_exact_named_caption_scope(self):
        spec=importlib.util.spec_from_file_location('startup_runner',Path(__file__).with_name('check-workflow-management-native.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
        for c in X.CASES:self.assertEqual(m.pointer_options('startup:'+c),dict(caption_timeout_ms=2000))
        for c in (None,'startup:unknown','startup','unknown'):self.assertEqual(m.pointer_options(c),{})
    def test_mode_requires_original_drafts_and_isolation(self):
        for c in X.CASES:X.Q.mode('startup:'+c,'success',True)
        for c in ('unknown',''):
            with self.assertRaises(RuntimeError):X.Q.mode('startup:'+c,'success',True)
        with self.assertRaises(RuntimeError):X.Q.mode('startup:success','success',False)
    def test_stage_order_acquires_before_modal(self):
        for c in X.CASES:
            stages=X.Q.STAGES['startup:'+c]
            if c=='socket-retired':
                self.assertEqual(stages,X.A.PREFIX+('startup-before','startup-retired'))
                continue
            self.assertLess(stages.index('startup-before'),stages.index('startup-offline'))
            self.assertLess(stages.index('startup-offline'),stages.index('share-open'))
            self.assertEqual(stages[-3:],('text-retained','offline-query','share-cancelled'))

class IsolatedRetirement(unittest.TestCase):
    def sample(self,case='prefs-retired'):
        row=sample(case,'startup-offline');before=sample(case,'startup-before')['model']['shareCustody']['startupObservations'];after=row['model']['shareCustody']['startupObservations']
        online=dict(routable=True,routeObserved=True,realtimeConnected=True,transportHealth='unknown')
        frame=dict(type='pref_change',name='display_real_names_override',value=1) if case=='prefs-retired' else dict(kind='closed-fixture-socket-generation-retirement',before=0,after=1,networkCalls=0)
        r=dict(scenario=case,generation=0,frame=frame,before=copy.deepcopy(before),after=copy.deepcopy(after),connectionBefore=copy.deepcopy(online),connectionAfter=copy.deepcopy(online),settledBeforeOffline=dict(observations=copy.deepcopy(after),connection=copy.deepcopy(online)),storeInjection=False)
        ex=dict(retirement=r,release=dict(serial=2,receiverRetired=False,delivered=True),transition=dict(kind='closed-fixture-connectivity-transition',owner=copy.deepcopy(X.S.OWNER),generation=0,currentGeneration=1 if case=='socket-retired' else 0,before=dict(routable=True,routeObserved=True,realtimeConnected=True),after=dict(routable=False,routeObserved=True,realtimeConnected=True,transportHealth='offline'),networkCalls=0,eventFabricated=False))
        if case=='socket-retired':
            r['settledOnline']=r.pop('settledBeforeOffline');ex['transition']=None
        return ex,row,before
    def test_actual_retirement_before_loss(self):
        for c in ('prefs-retired','socket-retired'):
            ex,row,before=self.sample(c);X.transition(c,ex,row,before)
    def test_offline_cannot_mask_bad_owner_guard(self):
        for key in ('connectionBefore','connectionAfter'):
            ex,row,before=self.sample();ex['retirement'][key]['routable']=False
            with self.assertRaises(RuntimeError):X.transition('prefs-retired',ex,row,before)
    def test_late_publication_before_offline_refused(self):
        ex,row,before=self.sample();ex['retirement']['settledBeforeOffline']['observations']['currentComplete']=True
        with self.assertRaises(RuntimeError):X.transition('prefs-retired',ex,row,before)
    def test_wrong_socket_successor_refused(self):
        ex,row,before=self.sample('socket-retired');ex['retirement']['frame']['after']=2
        with self.assertRaises(RuntimeError):X.transition('socket-retired',ex,row,before)


class RetiredManagement(unittest.TestCase):
    def sample(self):
        ids=dict(parent=1,title=2,description=3,modal=4,modalTitle=5,modalDescription=6,upload=7)
        native=dict(originalNodes=True,scope=dict(visit='1',document='12',revision='2'),fields=dict(title=dict(nodeId=2),description=dict(nodeId=3)),modal=dict(originalNodes=True,nodeId=4,owner='18',epoch='22',fields=dict(title=dict(nodeId=5),description=dict(nodeId=6)),upload=dict(nodeId=7)))
        binding=dict(ok=True,kind='original-startup-retirement-binding',scope=copy.deepcopy(native['scope']),modalOwner='18',modalEpoch='22',nodeIds=ids,windowOriginal=True)
        server=dict(owner=copy.deepcopy(X.S.OWNER),ownerCurrent=True,confirmationAdd=dict(share=dict(startup=dict(held=[2]),requests=[])))
        attempt=dict(id=18,visit=1,document=12,revision=2,current=False,finished=True,complete=True,closed=False,confirmationShare=None,publication=dict(raw=dict(ok=True,original=True)),metadata=[dict(raw=dict(ok=True))])
        before=dict(proof=dict(native=copy.deepcopy(native),exercise=dict(retirementBinding=binding),firstPublish=dict(visible=True,visit=1,attempts=[attempt]),server=server))
        row=copy.deepcopy(before);p=row['proof'];p['exercise']={};p['firstPublish']['visible']=False;p['firstPublish']['attempts'][0]['current']=False;p['server']['confirmationAdd']['share']['startup']['held']=[]
        proof=dict(kind='original-startup-management-retirement',originalObservationOk=False,binding=copy.deepcopy(binding),nodes=[dict(role=k,nodeId=v,connected=False) for k,v in ids.items()],registryOriginal=True,windowOriginal=True,noReplacement=True,shareEntryAbsent=True,actionDispatched=False)
        p['native']=dict(ok=True,present=False,originalDetached=True,originalNodes=None,leasePresent=False,scope=None,fields=None,modal=None,share=None,noOverflow=True,composing=False,controls=[],feedback=[],outcomes=[],startupRetirement=proof)
        return row,before,dict(proof=dict(native=native,firstPublish=copy.deepcopy(before['proof']['firstPublish'])))
    def reject(self,mutate):
        row,before,checked=self.sample();mutate(row,before,checked)
        with self.assertRaises(RuntimeError):X.retired_state(row,before,checked)
    def test_actual_retired_predicate_is_separate_from_normal_observation(self):X.retired_state(*self.sample())
    def test_normal_predicate_cannot_be_rewritten_success(self):self.reject(lambda r,b,c:r['proof']['native']['startupRetirement'].update(originalObservationOk=True))
    def test_no_retired_lease_grant(self):self.reject(lambda r,b,c:r['proof']['native'].update(leasePresent=True))
    def test_no_original_nodes_grant(self):self.reject(lambda r,b,c:r['proof']['native'].update(originalNodes=True))
    def test_no_replacement_modal(self):self.reject(lambda r,b,c:r['proof']['native'].update(modal={}))
    def test_no_replacement_management(self):self.reject(lambda r,b,c:r['proof']['native'].update(present=True))
    def test_no_replacement_native_node(self):self.reject(lambda r,b,c:r['proof']['native']['startupRetirement']['nodes'][1].update(nodeId=999))
    def test_original_fields_cannot_be_rebound(self):self.reject(lambda r,b,c:c['proof']['native']['fields']['title'].update(nodeId=999))
    def test_foreign_modal_owner_refused(self):self.reject(lambda r,b,c:b['proof']['exercise']['retirementBinding'].update(modalOwner='foreign'))
    def test_foreign_management_scope_refused(self):self.reject(lambda r,b,c:b['proof']['exercise']['retirementBinding']['scope'].update(visit='999'))
    def test_missing_window_proof_refused(self):self.reject(lambda r,b,c:r['proof']['native']['startupRetirement'].pop('windowOriginal'))
    def test_stale_action_refused(self):self.reject(lambda r,b,c:r['proof']['exercise'].update(activation=dict(ok=True)))
    def test_no_successor_attempt(self):self.reject(lambda r,b,c:r['proof']['firstPublish']['attempts'][0].update(id=19))
    def test_current_original_refused(self):self.reject(lambda r,b,c:r['proof']['firstPublish']['attempts'][0].update(current=True))
    def test_completed_original_is_already_not_current(self):
        row,before,checked=self.sample()
        self.assertFalse(before['proof']['firstPublish']['attempts'][0]['current'])
        self.assertEqual(row['proof']['firstPublish']['attempts'],before['proof']['firstPublish']['attempts'])
        X.retired_state(row,before,checked)
    def test_unfinished_original_refused(self):self.reject(lambda r,b,c:b['proof']['firstPublish']['attempts'][0].update(finished=False))
    def test_checked_history_replacement_refused(self):self.reject(lambda r,b,c:c['proof']['firstPublish']['attempts'][0].update(id=999))
    def test_extra_successor_history_refused(self):self.reject(lambda r,b,c:r['proof']['firstPublish']['attempts'].append(copy.deepcopy(r['proof']['firstPublish']['attempts'][0])))
    def test_raw_publication_immutable(self):self.reject(lambda r,b,c:r['proof']['firstPublish']['attempts'][0]['publication']['raw'].update(original=False))
    def test_no_hidden_share_mount(self):self.reject(lambda r,b,c:r['proof']['firstPublish']['attempts'][0].update(confirmationShare={}))
    def test_no_additional_server_request(self):self.reject(lambda r,b,c:r['proof']['server']['confirmationAdd']['share']['requests'].append(dict(method='chat.postMessage')))
    def test_foreign_server_owner_refused(self):self.reject(lambda r,b,c:r['proof']['server']['owner'].update(account='foreign'))

if __name__=='__main__':unittest.main()
