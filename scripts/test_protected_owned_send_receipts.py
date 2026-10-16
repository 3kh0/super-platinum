"""Public-oracle mutations; no desktop launch, network or real account data."""
import copy,json,runpy,tempfile,unittest
from pathlib import Path
from unittest.mock import patch

F=runpy.run_path(str(Path(__file__).with_name('owned_send_test_fixture.py')))
C=F['C'];R=runpy.run_path(str(Path(__file__).with_name('check-protected-owned-send-native.py')))
def good(*args):return json.loads(json.dumps(F['good'](*args)))
def validate(v):return F['validate'](v)


class Receipts(unittest.TestCase):
    def reject(self,edit,scenario='newer-before-prepare-channel'):
        value=good(scenario);edit(value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):validate(value)

    def test_all_four_complete_public_paths(self):
        for scenario in C['SCENARIOS']:
            for fixture in C['FIXTURES']:
                with self.subTest(scenario=scenario,fixture=fixture):validate(good(scenario,fixture))

    def test_exact_terminal_identity_and_no_extra_authority(self):
        for k,v in [('ok',1),('realHttpRequests',1),('preparedRequests',2),('actualOwnedRoute',False),('syntheticSession',False),('account',F['uid'](99)),('unexpectedCurrentPermit',True)]:
            self.reject(lambda r:r.update({k:v}))

    def test_actual_owned_ticket_and_file_ids(self):
        for key,value in [('version',True),('account',F['uid'](99)),('marker',[1,3]),('database',[1,True]),('nonce','../wrong'),('path','arbitrary')]:
            def edit(r):
                t=json.loads(r['ownedTicket']);t[key]=value;r['ownedTicket']=json.dumps(t)
            self.reject(edit)
        self.reject(lambda r:r.update(ownedTicket='{"version":1,"version":1}'))

    def test_full_initial_and_sibling_inventory(self):
        for field,value in [('clock',4),('uncommitted',C['local']('lost')),('origins',[{}]),('blocked',{})]:
            self.reject(lambda r:r['snapshots'][0]['durable']['scopes']['thread'].update({field:value}))
        self.reject(lambda r:r['snapshots'][-1]['durable']['scopes']['thread']['row']['state'].pop('check_ticket'))

    def test_frozen_consume_is_not_newer_content(self):
        self.reject(lambda r:r['snapshots'][2]['durable']['scopes']['channel']['row']['state']['consumption'].update(snapshot=C['encoded'](C['NEWER'],'channel',False)))
        self.reject(lambda r:r['snapshots'][2]['durable']['scopes']['channel']['row']['state']['consumption'].update(local_revision=3))
        self.reject(lambda r:r['snapshots'][3]['durable']['scopes']['channel']['row']['state'].update(workflow={'InFlight':r['attempts'][0]['operation']}))

    def test_record_archive_and_canonical_newer_pair(self):
        self.reject(lambda r:r['snapshots'][-1]['projection']['operation']['recorded']['row']['state'].pop('local_completion'))
        self.reject(lambda r:r['snapshots'][-1]['durable']['scopes']['channel']['local']['composer'].update(text=C['REQUESTED']))
        self.reject(lambda r:r['snapshots'][-1]['durable']['scopes']['channel'].update(clock=9))

    def test_exact_native_ownership_and_ack_sequence(self):
        for index in range(4):self.reject(lambda r:r['nativeAdmission'].pop(index))
        self.reject(lambda r:r['nativeAdmission'][2]['evidence']['after'].update(input=2))
        self.reject(lambda r:r['nativeAdmission'][1]['evidence'].update(token='8'))
        self.reject(lambda r:r['nativeAdmission'][0]['evidence']['before'].update(direction='forward'))
        self.reject(lambda r:r['snapshots'][-1]['geometry']['native'].update(start=0))

    def test_retained_parent_editor_writer_and_owner_cannot_change(self):
        for key,value in [('retentionId','protected-88'),('writerId',F['uid'](88)),('enrollment',F['uid'](88))]:
            self.reject(lambda r:r['snapshots'][3]['projection']['editors']['channel'].update({key:value}))
        for key,value in [('epoch',11),('sourceCurrent',False),('workerCurrent',False)]:
            self.reject(lambda r:r['snapshots'][3]['projection'].update({key:value}))
        self.reject(lambda r:r['snapshots'][-1]['projection']['operation'].update(id=2))

    def test_request_preserves_every_form_and_header(self):
        self.reject(lambda r:r['attempts'][0]['fields'][0].__setitem__(1,'[]'))
        self.reject(lambda r:r['attempts'][0]['fields'].append(['token','unexpected']))
        self.reject(lambda r:r['attempts'][0]['fields'].reverse())
        self.reject(lambda r:r['attempts'][0]['headers'][0].__setitem__(1,'"Windows"'))
        self.reject(lambda r:r['attempts'][0]['fields'][-1].__setitem__(1,'false'),'newer-before-prepare-thread')

    def test_retained_family_counts_and_no_unrelated_authority(self):
        self.reject(lambda r:r['snapshots'][-1]['projection']['retainedJobs'].append(dict(family='protected-release',generation=1)))
        self.reject(lambda r:r['snapshots'][3]['projection']['retainedJobs'].append(dict(family='protected-start-local',generation=1)))
        self.reject(lambda r:r['snapshots'][0]['projection']['retainedJobs'].append(dict(family='protected-slash-claim',generation=1)))
        self.reject(lambda r:r['snapshots'][2]['projection']['retainedJobs'].pop())
        self.reject(lambda r:r['snapshots'][0]['projection']['retainedJobs'][0].update(generation=2))

    def test_url_default_pairs_and_only_valid_volatile_uuid_values(self):
        for suffix in ['&_x_id=duplicate','&unknown=1','#fragment']:
            self.reject(lambda r:r['attempts'][0].update(url=r['attempts'][0]['url']+suffix))
        for before,after in [('_x_num_retries=0','_x_num_retries=1'),('0123456789ab4def','0123456789ab5def'),('owned-send.slack.com','evil.invalid')]:
            self.reject(lambda r:r['attempts'][0].update(url=r['attempts'][0]['url'].replace(before,after)))
        def aliases(r):
            from urllib.parse import urlsplit,parse_qsl,urlencode
            url=r['attempts'][0]['url'];parsed=urlsplit(url);pairs=parse_qsl(parsed.query)
            first=dict(pairs)['_x_id']
            r['attempts'][0]['url']=url.split('?')[0]+'?'+urlencode([(k,first if k in ('_x_csid','fp') else v) for k,v in pairs])
        self.reject(aliases)

    def test_full_raw_and_actual_decoder_parent_retention(self):
        self.reject(lambda r:r['attempts'][0]['rawResponse']['message'].update(client_msg_id=F['uid'](90)))
        self.reject(lambda r:r['snapshots'][-1]['projection']['operation']['attempt']['raw'].update(extra='counterfeit'))
        self.reject(lambda r:r['snapshots'][-1]['projection']['operation']['attempt'].update(dispatched=False))
        self.reject(lambda r:r['snapshots'][-1]['projection']['operation']['attempt'].update(outcome='Rejected'))

    def test_no_prepare_http_or_publication_before_hold_release(self):
        self.reject(lambda r:r['snapshots'][3]['projection']['operation'].update(operation=r['attempts'][0]['operation']))
        self.reject(lambda r:r['snapshots'][2]['projection']['operation']['publication'].update(pendingSend=True))
        self.reject(lambda r:r['snapshots'][3]['projection']['operation'].update(releasePresent=True))

    def test_geometry_and_native_home_anchor(self):
        self.reject(lambda r:r['snapshots'][0]['geometry']['hitSamples']['field']['points'][0].update(owned=False))
        self.reject(lambda r:r['details']['send']['hitSamples']['button']['points'].pop())
        self.reject(lambda r:r['snapshots'][1]['geometry']['field'].update(x=-1))
        self.reject(lambda r:r['details']['nativeInitial']['homeAnchor']['after'].update(atBottom=False))
        for field in ('scope','visit'):
            self.reject(lambda r:r['details']['send'].update({field:'foreign-existing-mount'}))

    def test_readiness_tuple_includes_exact_fixture(self):
        expected=R['ready_tuple'](F['NONCE'],123,C['SCENARIOS'][0],'exercise',C['FIXTURES'][0])
        R['validate_ready'](expected,expected)
        for key,value in [('fixture',C['FIXTURES'][1]),('pid',True),('phase','restart')]:
            bad=dict(expected);bad[key]=value
            with self.assertRaises(RuntimeError):R['validate_ready'](bad,expected)

    def test_capture_paths_and_complete_ordered_edges_are_owned(self):
        value=good();directory=F['STORAGE'].parent
        names=C['stages'](C['SCENARIOS'][0]);edges=[dict(stage=s,edge=e) for s in names for e in ('before','after')]
        value['captures']={s:str(directory/(s+'.png')) for s in names}
        def check(v,es):
            return R['validate_artifacts'](C['FIXTURES'][0],C['SCENARIOS'][0],'exercise',F['NONCE'],123,
                                           F['STORAGE'],directory,v,es,[100,100])
        with patch.dict(R['validate_artifacts'].__globals__,png=lambda path,dimensions:[100,100]):
            self.assertEqual(set(check(value,edges)),set(names))
            bad=copy.deepcopy(value);bad['captures']['initial']=str(directory/'../foreign.png')
            with self.assertRaises(RuntimeError):check(bad,edges)
            for wrong in (edges[:-1],edges[::-1],edges+[edges[-1]]):
                with self.assertRaises(RuntimeError):check(value,wrong)

    def test_staged_runner_cannot_launch(self):
        self.assertEqual(C['CONTRACT_READY'],Path(__file__).resolve().parent==C['SCRIPTS'])
        with patch.dict(R['require_contract'].__globals__,CONTRACT_READY=False):
            with self.assertRaises(RuntimeError):R['require_contract']()
            with self.assertRaises(RuntimeError):R['run'](C['FIXTURES'][0],C['SCENARIOS'][0],Path('none'),Path('none'))


if __name__=='__main__':unittest.main()
