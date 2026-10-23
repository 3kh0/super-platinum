"""Portable original-ledger and source-custody mutations; no native claims."""
import copy
import importlib.util
import unittest
from pathlib import Path
import workflow_management_share_empty as X


def server(case='initial',stage='share-open'):
    prefix=stage in X.A.PREFIX
    v=dict(case='existing-search',navigationOwner='12345678-1234-4234-8234-123456789abc',networkCalls=0,cacheInjection=False,canonicalWrites=0,localPeople=None,modelMode='actual-read-absent-model',requests=[],responses=[],settlements=[])
    me=dict(id='UWORKFLOW',team_id='TWORKFLOW')
    methods=[] if prefix or case=='cold-offline' else ['client.gantryBoot','users.prefs.get']+([] if case in X.COLD else ['experiments.getByUser','search.autocomplete.model','search.autocomplete.offlineFeatures'])
    for n,method in enumerate(methods,1):
        body=X.R.READ_BODIES.get(method,[['ssb_platform','windows'],['ssb_release_channel','prod']])
        v['requests'].append(dict(serial=n,kind='cache' if method.startswith('search.') else 'query',method=method,body=body,endpoint='https://example.test/api/'+method,retrySafe=False,owner=copy.deepcopy(X.S.OWNER)))
        raw=X.R.observation_reply(method,'cold-mixed',me)
        if case=='cold-missing' and method=='users.prefs.get':raw=dict(ok=True,opaque_receipt='original-empty-query-missing-prefs')
        v['responses'].append(dict(serial=n,error='offline') if case=='cold-failure' and method=='users.prefs.get' else dict(serial=n,raw=raw,orderedKeys=None))
        v['settlements'].append(dict(serial=n,returned=True))
    connection=dict(owner=copy.deepcopy(X.S.OWNER),generation=0,carrier='owned-in-memory-realtime-channel',receiverClosed=False,queuedFrames=0,networkCalls=0,httpSuccessClaimed=False,installed=dict(routeObserved=True,routable=case!='cold-offline',realtimeConnected=True,transportHealth='offline' if case=='cold-offline' else 'unknown'))
    d=dict(case='empty',emptyCase=case,initialPeople=75,networkCalls=0,remotePersonInitiallyAbsent=True,remoteChannelInitiallyAbsent=True,postingSources=[],requests=[],responses=[],settlements=[],releases=[],held=[],ranking=v)
    parent=dict(search=d,connectivity=connection,reads=[],requests=[],responses=[],attempts=[],settlements=[],releases=[],held=[],applied=[],started=False)
    return dict(seed=dict(boot=dict(self=me)),confirmationAdd=dict(share=parent))


def remote(case,stage):
    s=server(case,stage);v=s['confirmationAdd']['share']['search'];q='review' if case=='clear' else ' '
    pending=stage in ('query-pending','whitespace-pending');released=not pending and not(case=='clear' and stage=='empty-cleared')
    methods=['api.getFlannelHttpUrl','users/search']+(['channels/search'] if case=='prefixes' and not pending else [])
    for n,method in enumerate(methods,1):
        discovery=method=='api.getFlannelHttpUrl'
        v['requests'].append(dict(serial=n,method=method,query='' if discovery else q,body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else X.Q.body(method,q),endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method,retrySafe=False,owner=copy.deepcopy(X.S.OWNER)))
        raw=X.F.DISCOVERY_RAW if discovery else dict(ok=True,results=[],next_marker='',opaque_receipt=dict(fixture='original-empty-query-search',query=q,method=method))
        v['responses'].append(dict(serial=n,raw=raw,orderedKeys=None))
        if not(n==2 and pending):v['settlements'].append(dict(serial=n,returned=not(case=='clear' and n==2)))
    v['held']=[] if released else [2]
    v['releases']=[dict(serial=2,receiverRetired=case=='clear',delivered=case!='clear')] if released else []
    return s


class EmptyObservations(unittest.TestCase):
    def test_successful_fresh_and_warm_originals(self):
        for case in X.CASES:
            s=server(case);previous=X.observation_ledger(case,'share-open',s)
            X.observation_ledger(case,'share-cancelled',s,previous)
    def test_prefix_before_mount_has_no_observation_requests(self):
        for case in X.CASES:X.observation_ledger(case,'checked',server(case,'checked'))
    def reject(self,mutate,case='initial'):
        s=server(case);mutate(s['confirmationAdd']['share'])
        with self.assertRaises(RuntimeError):X.observation_ledger(case,'share-open',s)
    def test_wrong_original_owner(self):self.reject(lambda p:p['search']['ranking']['requests'][0]['owner'].update(account='other'))
    def test_fake_complete_missing_preferences(self):self.reject(lambda p:p['search']['ranking']['responses'][1].update(raw=dict(ok=True,prefs={})),case='cold-missing')
    def test_offline_read_is_not_empty_preferences(self):self.reject(lambda p:p['search']['ranking']['responses'][1].update(raw=dict(ok=True,prefs={})),case='cold-failure')
    def test_no_cold_offline_acquisition(self):self.reject(lambda p:p['search']['ranking']['requests'].append(dict(kind='query',method='users.prefs.get')),case='cold-offline')
    def test_no_fake_http_success(self):self.reject(lambda p:p['connectivity'].update(httpSuccessClaimed=True))
    def test_socket_receiver_custody(self):self.reject(lambda p:p['connectivity'].update(receiverClosed=True))
    def test_route_must_be_observed(self):self.reject(lambda p:p['connectivity']['installed'].update(routeObserved=False))
    def test_navigation_namespace_is_not_permission_owner(self):self.reject(lambda p:p['search']['ranking'].update(navigationOwner=X.S.OWNER['account']))
    def test_missing_order_is_not_empty_map_order(self):self.reject(lambda p:p['search']['ranking']['responses'][-1].update(orderedKeys=[]))
    def test_old_original_observation_immutable(self):
        s=server();old=copy.deepcopy(s['confirmationAdd']['share']['search']['ranking']);old['responses'][0]['raw']['opaque_receipt']='other'
        with self.assertRaises(RuntimeError):X.observation_ledger('initial','share-open',s,old)


class EmptyDirectory(unittest.TestCase):
    def test_no_directory_read_for_local_cases(self):
        for case in X.CASES:X.directory_ledger(case,'share-open',server(case))
    def test_exact_whitespace_and_retired_original(self):
        for case,stages in [('clear',('query-pending','empty-cleared','late-settled','empty-refreshed','share-cancelled')),('prefixes',('whitespace-pending','whitespace-ready','empty-cleared','share-cancelled'))]:
            for stage in stages:X.directory_ledger(case,stage,remote(case,stage))
    def reject(self,mutate,case='prefixes',stage='whitespace-ready'):
        s=remote(case,stage);mutate(s['confirmationAdd']['share']['search'])
        with self.assertRaises(RuntimeError):X.directory_ledger(case,stage,s)
    def test_whitespace_must_not_trim_wire(self):self.reject(lambda v:v['requests'][1]['body'].update(query=''))
    def test_no_published_late_old_response(self):self.reject(lambda v:v['settlements'][1].update(returned=True),'clear','late-settled')
    def test_wrong_held_query_serial(self):self.reject(lambda v:v.update(held=[1]),'clear','query-pending')
    def test_no_source_order_invention(self):self.reject(lambda v:v['responses'][1].update(orderedKeys=[]))
    def test_no_query_extra_request(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][1])))
    def test_no_send_or_open(self):
        for key in ('requests','responses','attempts','settlements','releases','held','applied'):
            s=server();s['confirmationAdd']['share'][key].append({'synthetic':'unexpected'})
            with self.assertRaises(RuntimeError):X.ordinary('share-open',s)
    def test_cold_mode_and_mutual_exclusion(self):
        for case in X.CASES:X.Q.mode('empty:'+case,'success',True)
        with self.assertRaises(RuntimeError):X.Q.mode('empty:initial','success',False)
        with self.assertRaises(RuntimeError):X.Q.mode('empty:initial','success',True,False,None,None,'success')
    def test_only_exact_case_caption_budget(self):
        path=Path(__file__).with_name('check-workflow-management-native.py');spec=importlib.util.spec_from_file_location('empty_runner',path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
        for case in X.CASES:self.assertEqual(m.pointer_options('empty:'+case),dict(caption_timeout_ms=2000))
        for case in (None,'empty:unknown','person-success-other'):self.assertEqual(m.pointer_options(case),{})


class PostingSubset(unittest.TestCase):
    def sample(self):
        readonly=copy.deepcopy(X.A.CHANNELS['CADDREAD01'])
        s=server();s['seed']['boot']['channels']=[readonly]
        s['confirmationAdd']['share']['reads']=[dict(result=dict(raw=dict(channel=readonly)))]
        search=dict(ranking=dict(results=[dict(id='CADDREAD01'),dict(id=X.A.DEST)]),results=[X.A.DEST],status='partial',failures=[dict(channel='CADDREAD01',stage='posting',failure='destination unavailable')])
        return s,search
    def test_exact_source_rank_vs_admitted_subset(self):
        s,q=self.sample();X.posting_subset(q,s)
    def test_reject_arbitrary_partial(self):
        s,q=self.sample();q['failures'][0]['channel']=X.A.DEST
        with self.assertRaises(RuntimeError):X.posting_subset(q,s)
    def test_reject_readonly_grant(self):
        s,q=self.sample();q['results'].insert(0,'CADDREAD01')
        with self.assertRaises(RuntimeError):X.posting_subset(q,s)
    def test_reject_no_posting_source(self):
        s,q=self.sample();s['confirmationAdd']['share']['reads']=[]
        with self.assertRaises(RuntimeError):X.posting_subset(q,s)
    def test_reject_early_ranking_drop(self):
        s,q=self.sample();q['ranking']['results'].pop(0)
        with self.assertRaises(RuntimeError):X.posting_subset(q,s)

class OfflineTransition(unittest.TestCase):
    def witness(self):
        return dict(kind='closed-fixture-connectivity-transition',owner=copy.deepcopy(X.S.OWNER),generation=0,before=dict(routable=True,routeObserved=True,realtimeConnected=True),after=dict(routable=False,routeObserved=True,realtimeConnected=True,transportHealth='offline'),networkCalls=0,eventFabricated=False)
    def test_owned_actual_route_health_transition(self):X.offline_transition(self.witness(),server('warm-offline'))
    def test_no_fake_event_wrong_owner_or_online_verdict(self):
        for mutate in (lambda v:v.update(eventFabricated=True),lambda v:v['owner'].update(account='other'),lambda v:v['after'].update(transportHealth='online'),lambda v:v.update(generation=2)):
            v=self.witness();mutate(v)
            with self.assertRaises(RuntimeError):X.offline_transition(v,server('warm-offline'))

class CurrentReadyNotice(unittest.TestCase):
    def test_successful_ready_and_partial_have_no_legacy_notice(self):
        for status in ('ready','partial'):
            X.successful_notice(dict(search=dict(status=status),notice=None),dict(share=dict(notice=None)))
    def test_stale_model_or_original_dom_notice_refused(self):
        stale='Some destinations could not be verified. Refresh to try again.'
        for model,dom in ((stale,None),(None,stale),(stale,stale)):
            with self.assertRaises(RuntimeError):X.successful_notice(dict(search=dict(status='ready'),notice=model),dict(share=dict(notice=dom)))
    def test_failed_ranking_does_not_fabricate_notice_clear(self):
        X.successful_notice(dict(search=dict(status='failed'),notice='original failure'),dict(share=dict(notice='original failure')))
    def test_filtered_choices_precede_original_selection(self):
        for case in ('clear','prefixes','warm-offline'):
            stages=X.Q.STAGES['empty:'+case]
            self.assertEqual(stages[stages.index('share-open')+1:stages.index('selection-retained')+1],('channel-choices','selection-retained'))

if __name__=='__main__':unittest.main()
