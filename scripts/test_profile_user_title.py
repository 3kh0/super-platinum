"""Portable adversarial receipt tests. Constructed receipts are not native evidence."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import profile_user_title as O
from test_profile_file_titles_native import sample as old_sample, R

RUN=runpy.run_path(str(Path(__file__).with_name('check-profile-user-title-native.py')))
DIRECTORY=Path(tempfile.gettempdir())/'profile-user-title-validator-only'


def sample(case='recovery',narrow=False,retired=False):
    # Reuse the established outer editor shape, then construct the new source
    # timeline from the documented fixture ABI, independently of validate().
    R['MODE'].update(list=False,blocks=True)
    fixture,old=old_sample(DIRECTORY,narrow)
    original={r['stage']:r for r in old['stages']}
    result=copy.deepcopy(old)
    result.update(userTitle=True,case=case,stages=[],captures={})
    for stage in O.STAGES[case]:
        ready=stage in ('opened','confirmed');known=case in ('known','hidden')
        mapped=stage if stage in original else 'held'
        row=copy.deepcopy(original[mapped]);row['stage']=stage
        model=row['model'];model.update(serial=4 if stage=='confirmed' else 3,blocks=O.blocks(stage=='confirmed'),
            reads=int(stage=='confirmed'),attempts=[],canSave=False)
        if stage=='confirmed':
            model['attempts']=[[dict(element_id='long',long_text=dict(blocks=json.dumps(O.blocks(True)))),
                dict(element_id='locked',text=dict(text='Preserved organization value'))]]
        g=row['geometry'];g.update(tokens=[],captured=None,richEditable=True if ready else None)
        controls=['Back','Save section']+(['Use description','Cancel description edit'] if ready else
                 ['Retry description','Cancel description edit'] if stage!='cancelled' else [])
        rect=dict(left=120,top=180,right=360,bottom=204,width=240,height=24)
        g['controls']=[dict(text=c,disabled=c=='Save section' or (c=='Retry description' and stage!='unavailable'),
                            visible=True,rect=rect.copy()) for c in controls]
        notice=O.LOADING if stage in O.HELD else O.UNAVAILABLE if stage=='unavailable' else 'Profile fields saved.' if stage=='confirmed' else None
        g['notices']=[dict(text=notice,visible=True,rect=rect.copy())] if notice else []
        if ready:
            g['tokens']=[dict(text=O.label(case),editable=False,fontWeight=w,fontStyle=s,visible=True,rect=rect.copy())
                         for w,s in [('700','normal'),('400','italic')]]
            g['captured']=dict(snapshot=dict(blocks=O.blocks(stage=='confirmed'),generation='22',revision=1,composing=False),
                tokens=[dict(value=t,label=O.label(case)) for t in O.blocks()[0]['elements'][0]['elements'][1:]])
        exercise={'cold-loading':dict(cold=True),'member-discovery':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
            'member-held':dict(serial=2),'unavailable':None,'retry-member-discovery':dict(serial=3,fileReused=True),
            'retry-member-held':dict(serial=4,fileReused=True),
            'opened':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
            'confirmed':dict(confirmedSave=True,fileReused=True),
            'cancelled':dict(release=dict(serial=2,receiverRetired=retired,delivered=not retired))}[stage]
        requests=[]
        count=0 if known or stage=='cold-loading' else 1 if stage=='member-discovery' else 3 if stage=='retry-member-discovery' else 4 if case=='failure' and stage in ('retry-member-held','opened','confirmed') else 2
        for serial in range(1,count+1):
            discovery=serial in (1,3)
            pending=(stage,serial) in [('member-discovery',1),('member-held',2),('retry-member-discovery',3),('retry-member-held',4)]
            request=dict(serial=serial,kind='discovery' if discovery else 'member',method='api.getFlannelHttpUrl' if discovery else 'users/info',
                body=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                    dict(updated_ids={'UMISSING01':0},check_interaction=True,include_profile_only_users=True),
                account=O.ACCOUNT,team=O.TEAM,epoch=model['epoch'],released=not pending)
            if not pending:
                failed=case=='failure' and serial==2
                request.update(outcome='offline' if failed else 'response',receiverRetired=False,delivered=True,
                    response=dict(error='offline') if failed else dict(raw=dict(ok=True,url='https://edgeapi.slack.com/cache/TINIT0001',ttl_seconds=300)
                        if discovery else O.member_response(),disconnected=None))
                if stage=='cancelled' and serial==2:request.update(receiverRetired=retired,delivered=not retired)
            requests.append(request)
        active=int(stage in ('member-discovery','member-held','retry-member-discovery','retry-member-held'))
        started=0 if count==0 else 2 if count>=3 else 1
        member=dict(profile=copy.deepcopy(model),account=O.ACCOUNT,epoch=model['epoch'],sourcePresent=known or ready,
            sourceObserved=True,native=O.member_raw() if known or ready else None,requests=requests,
            pending=[copy.deepcopy(r) for r in requests if not r['released']],driver=dict(started=started,finished=started-active,active=active))
        source=dict(case=case,seed=O.seed(case),files=dict(requests=[dict(serial=1,method='files.info',args=dict(file='FTITLE0001',page='1',count='500',truncate='true'))],
            completed=int(stage!='cold-loading'),cancelled=0,held=stage=='cold-loading',active=int(stage=='cold-loading'),networkCalls=0),
            member=member,policy=[dict(serial=1,method='users.prefs.get',body={},account=O.ACCOUNT,team=O.TEAM,user=O.SELF,
                                     epoch=model['epoch'],raw=O.policy(case))] if ready else [],networkCalls=0)
        cache=None
        if stage!='cold-loading':
            raw=dict(id='FTITLE0001',mode='quip',filetype='quip',created=101,updated=101,title='Misleading Canvas fallback',
                name='wrong-name-fallback',fixture_opaque=dict(preserve=['FTITLE0001',1,2]),title_blocks=O.title_blocks())
            effective=copy.deepcopy(raw);del effective['fixture_opaque']
            effective.update(is_deleted=False,is_tombstoned=False,isUnknown=False,isNonExistent=False)
            cache=dict(source=raw,effective=effective,current=ready,quipTitle=dict(current=True,value='nullish',fileId='FTITLE0001'),titleUsers=None)
            if ready:
                canonical=O.member_source();native=dict(O.member_raw(),is_stranger=False)
                if known:canonical.pop('is_stranger');native.pop('is_stranger')
                cache['titleUsers']=dict(members=[dict(source=canonical,effective=native)],
                    policy=dict(user=O.policy(case),enterprise=None,revision=0),generation='profile-canvas-title-users')
        row['proof']=dict(source=source,cache=cache,exercise=exercise)
        result['stages'].append(row)
        result['captures'][stage]=str(DIRECTORY.resolve()/f'{fixture}-{stage}.png')
    return fixture,result


class UserTitleTests(unittest.TestCase):
    def setUp(self):self.fixture,self.receipt=sample()
    def check(self):
        return O.validate(self.fixture,self.receipt,DIRECTORY,R['B'],case=self.receipt['case'],platform='win32',png=False)
    def row(self,stage):return next(r for r in self.receipt['stages'] if r['stage']==stage)
    def reject(self,change):
        change()
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError)):self.check()
    def test_all_cases_both_widths(self):
        for case in O.STAGES:
            for narrow in (False,True):
                self.fixture,self.receipt=sample(case,narrow)
                self.check()
    def test_cancel_actual_shared_receiver_both_dispositions(self):
        for retired in (False,True):
            self.fixture,self.receipt=sample('cancel',retired=retired);self.check()
    def test_raw_boot_cannot_promote_absent_member(self):
        self.reject(lambda:self.row('cold-loading')['proof']['source']['seed']['boot']['users'].append(O.member_raw()))
    def test_known_is_not_a_member_request(self):
        self.fixture,self.receipt=sample('known')
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'].append({}))
    def test_nested_id_dedup_not_two_reads(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'].append(copy.deepcopy(self.row('opened')['proof']['source']['member']['requests'][-1])))
    def test_member_request_original_owner(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1].update(account='foreign'))
    def test_member_request_exact_generation(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1].update(epoch=99))
    def test_member_factory_exact_updated_ids(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1]['body'].update(updated_ids={'UOTHER':0}))
    def test_member_factory_interaction_required(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1]['body'].update(check_interaction=False))
    def test_member_response_is_original(self):
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1]['response']['raw']['results'][0].update(id='UOTHER'))
    def test_no_early_source_from_held_response(self):
        self.reject(lambda:self.row('member-held')['proof']['source']['member'].update(sourcePresent=True))
    def test_no_early_policy(self):
        self.reject(lambda:self.row('member-held')['proof']['source'].update(policy=self.row('opened')['proof']['source']['policy']))
    def test_policy_exact_owner(self):
        self.reject(lambda:self.row('opened')['proof']['source']['policy'][0].update(user='UOTHER'))
    def test_policy_same_original_raw(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['titleUsers']['policy']['user']['prefs'].update(display_real_names_override=0))
    def test_hidden_requires_original_observation(self):
        self.fixture,self.receipt=sample('hidden')
        self.reject(lambda:self.row('opened')['proof']['source']['policy'][0]['raw']['prefs'].update(hidden_users=''))
    def test_hidden_render_not_display_name(self):
        self.fixture,self.receipt=sample('hidden')
        self.reject(lambda:self.row('opened')['geometry']['tokens'][0].update(text=O.label('known')))
    def test_complete_policy_not_reacquired_after_save(self):
        self.reject(lambda:self.row('confirmed')['proof']['source']['policy'].append(copy.deepcopy(self.row('confirmed')['proof']['source']['policy'][0])))
    def test_member_canonical_not_native_only(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['titleUsers']['members'][0].update(source=O.member_raw()))
    def test_stale_file_does_not_authorize_title(self):
        self.reject(lambda:self.row('opened')['proof']['cache'].update(current=False))
    def test_original_file_not_foreign(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['source'].update(id='FFOREIGN'))
    def test_duplicate_nested_user_retains_style(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['source']['title_blocks'][0]['elements'][0]['elements'][3].pop('style'))
    def test_original_opaque_not_discarded(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['effective']['title_blocks'][0].pop('opaque_block'))
    def test_user_name_not_username_or_realname_policy(self):
        for wrong in ('Review with @username-must-not-win','Review with @Profile fallback'):
            self.fixture,self.receipt=sample()
            self.reject(lambda:self.row('opened')['geometry']['tokens'][0].update(text=wrong))
    def test_literal_entity_is_not_unescaped(self):
        self.reject(lambda:self.row('opened')['geometry']['tokens'][0].update(text=O.label('known').replace('&amp;','&')))
    def test_native_quip_override_not_fabricated(self):
        self.reject(lambda:self.row('opened')['proof']['cache']['quipTitle'].update(value='text'))
    def test_native_raw_url_not_label(self):
        self.reject(lambda:self.row('opened')['geometry']['captured']['tokens'][0]['value'].update(text=O.label('known')))
    def test_outer_bold_render_custody(self):
        self.reject(lambda:self.row('opened')['geometry']['tokens'][0].update(fontWeight='400'))
    def test_editor_composition_rejected(self):
        self.reject(lambda:self.row('opened')['geometry']['captured']['snapshot'].update(composing=True))
    def test_wire_original_blocks(self):
        self.reject(lambda:self.row('confirmed')['model']['attempts'][0][0]['long_text'].update(blocks='[]'))
    def test_save_is_single(self):
        self.reject(lambda:self.row('confirmed')['model']['attempts'].append(copy.deepcopy(self.row('confirmed')['model']['attempts'][0])))
    def test_save_readback_original_section(self):
        self.reject(lambda:self.row('confirmed')['model'].update(serial=3))
    def test_sibling_custody(self):
        self.reject(lambda:self.row('opened')['model'].update(countryValue='FR'))
    def test_retry_reuses_file(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-held')['proof']['source']['files']['requests'].append({}))
    def test_retry_retains_offline_receipt(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'][1].update(response={'raw':O.member_response()}))
    def test_retry_requires_new_dispatch_discovery(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-held')['proof']['source']['member']['requests'][2].update(kind='member'))
    def test_retry_discovery_original_factory(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-discovery')['proof']['source']['member']['requests'][2]['body'].update(include_external_workspaces='false'))
    def test_retry_discovery_must_remain_pending_until_release(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-discovery')['proof']['source']['member']['requests'][2].update(released=True))
    def test_retry_discovery_original_response(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-held')['proof']['source']['member']['requests'][2]['response']['raw'].update(url='https://foreign.invalid/cache/TINIT0001'))
    def test_retry_member_is_serial_four(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('retry-member-held')['proof']['exercise'].update(serial=3))
    def test_retry_no_fifth_request(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.row('opened')['proof']['source']['member']['requests'].append({}))
    def test_retry_member_waits_for_discovery_capture(self):
        self.fixture,self.receipt=sample('failure')
        self.reject(lambda:self.receipt['stages'].remove(self.row('retry-member-discovery')))
    def test_actual_failed_native_receipt_requires_discovery_three(self):
        diagnostic=Path('tmp/profile-user-title-20261022/oracle/retry-discovery-correction/diagnostic.json')
        actual=json.loads(diagnostic.read_text(encoding='utf-8'))
        self.assertFalse(actual['nativeOk'])
        self.assertEqual(actual['result']['exitCode'],1)
        self.assertEqual(actual['requests'],O.member_requests('failure','retry-member-discovery',0))
        self.assertEqual(actual['driver'],dict(started=2,finished=1,active=1))
        self.assertEqual(actual['files'],O.file_source('retry-member-discovery'))
        self.assertEqual(actual['policy'],[])
    def test_cancel_does_not_adopt_delivered_reply(self):
        self.fixture,self.receipt=sample('cancel')
        self.reject(lambda:self.row('cancelled')['proof']['source']['member'].update(sourcePresent=True,native=O.member_raw()))
    def test_cancel_no_policy(self):
        self.fixture,self.receipt=sample('cancel')
        self.reject(lambda:self.row('cancelled')['proof']['source']['policy'].append({}))
    def test_cancel_receiver_fact_not_rewritten(self):
        self.fixture,self.receipt=sample('cancel')
        self.reject(lambda:self.row('cancelled')['proof']['exercise']['release'].update(delivered=False))
    def test_cancel_driver_must_settle(self):
        self.fixture,self.receipt=sample('cancel')
        self.reject(lambda:self.row('cancelled')['proof']['source']['member']['driver'].update(active=1))
    def test_capture_order(self):
        self.reject(lambda:self.receipt['stages'].reverse())
    def test_capture_foreign_path(self):
        self.reject(lambda:self.receipt['captures'].update(opened=str(DIRECTORY/'other.png')))
    def test_hidden_native_control(self):
        self.reject(lambda:self.row('opened')['geometry']['controls'][0].update(visible=False))
    def test_narrow_clipped_token(self):
        self.fixture,self.receipt=sample(narrow=True)
        self.reject(lambda:self.row('opened')['geometry']['tokens'][0]['rect'].update(right=900,width=780))
    def test_no_live_fallback(self):
        self.reject(lambda:self.row('opened')['proof']['source'].update(networkCalls=1))
    def test_wrapper_case_closed(self):
        with self.assertRaises(RuntimeError):RUN['harness']('other')
    def test_wrapper_uses_legacy_closed_guards(self):
        base=RUN['harness']('known')
        self.assertEqual(base['PREFIX'],'profile-user-title-check: ')
        self.assertEqual(base['stages'](),('cold-loading','opened','confirmed'))
        self.assertIs(base['validate_closed'].__globals__,base)
        self.assertEqual(base['EDGE'],'profile-sections-mentions-capture: ')
        self.assertEqual(R['B']['PREFIX'],'profile-file-titles-check: ')
    def test_wrapper_exact_flags_without_launch(self):
        base=RUN['harness']('failure');cls=base['NativeFixture'];file_cls=cls.__bases__[0]
        native_cls=file_cls.__bases__[0]
        calls=[]
        with patch.object(native_cls,'__init__',lambda self,*args:calls.append(args)):
            cls('profile-sections-mentions',Path('x.log'),Path('x.exe'),{})
        flags=calls[0][-1]
        for key,value in {'SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK':'1','SUPER_PLATINUM_PROFILE_USER_TITLE_CASE':'failure',
                          'SUPER_PLATINUM_PROFILE_FILE_TITLES_CHECK':'1','SUPER_PLATINUM_PROFILE_FILE_TITLE_BLOCKS':'1',
                          'SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND':'canvas'}.items():self.assertEqual(flags[key],value)
    def test_wrapper_cli_rejects_unbounded_case(self):
        run=subprocess.run([sys.executable,str(Path(__file__).with_name('check-profile-user-title-native.py')),
            '--case','other','--captures',str(DIRECTORY)],capture_output=True,text=True)
        self.assertEqual(run.returncode,2)


if __name__=='__main__':unittest.main()
