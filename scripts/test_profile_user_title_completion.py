"""Portable receipt mutations; synthetic vectors do not claim native acceptance."""
import copy
import json
from pathlib import Path
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch
import profile_user_title as U
import profile_user_title_completion as O
import profile_user_title_completion_source as S

R=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
RUN=runpy.run_path(str(Path(__file__).with_name('check-profile-user-title-completion-native.py')))
PATH=Path(tempfile.gettempdir())/'profile-user-title-completion-tests'
STAGES={
 'known':['opened','query-ready','inserted','saved'],
 'hidden':['opened','query-ready','inserted','saved'],
 'recovery':['opened','member-discovery','member-held','unavailable','retry-member-discovery','retry-member-held','query-ready','inserted','saved'],
 'retirement':['opened','member-discovery','member-held','query-replaced','late-settled','replacement-member-discovery','replacement-member-held','cancelled']}


def raw_bytes():
    return S.BASE_BYTES.read_bytes().decode().replace('"title":"Alex Canvas"','"title":"Alex Canvas","title_blocks":'+json.dumps(U.title_blocks(),separators=(',',':'),sort_keys=True),1)


def rect(left=120,top=160,width=240,height=18):
    return dict(left=left,top=top,width=width,height=height,right=left+width,bottom=top+height)


def member_requests(count,pending,case,retired):
    rows=[]
    for serial in range(1,count+1):
        discovery=serial in (1,3)
        r=dict(serial=serial,kind='discovery' if discovery else 'member',method='api.getFlannelHttpUrl' if discovery else 'users/info',
            body=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                dict(updated_ids={'UMISSING01':0},check_interaction=True,include_profile_only_users=True),
            account=U.ACCOUNT,team=U.TEAM,epoch=0,released=serial!=pending)
        if serial!=pending:
            offline=case=='recovery' and serial==2
            dropped=retired if case=='retirement' and not discovery else False
            r.update(outcome='offline' if offline else 'response',receiverRetired=dropped,delivered=not dropped,
                response=dict(error='offline') if offline else dict(raw=dict(ok=True,url='https://edgeapi.slack.com/cache/TINIT0001',ttl_seconds=300)
                    if discovery else U.member_response(),disconnected=None))
        rows.append(r)
    return rows


def sample(case='known',narrow=False,retired=False):
    fixture=R['FIXTURES'][int(narrow)];viewport=[668,380] if narrow else [1280,780]
    initial=R['initial']();token=R['token']('FCOMPLETE1');wire=copy.deepcopy(initial)
    wire[0]['elements'][0]['elements'] += [dict(type='text',text=' '),token,dict(type='text',text=' ')]
    raw=raw_bytes();files=json.loads(raw)['files'];gantry=dict(ok=True,self=dict(id=U.SELF),team=dict(id=U.TEAM),workspaces=[dict(id=U.TEAM)])
    result=dict(ok=True,fixture=fixture,case=case,initialBlocks=initial,stages=[],captures={})
    if case!='retirement':result['savedBlocks']=wire
    requests=[];seen=[];ready_choices=None;replacement=None
    for stage in STAGES[case]:
        saved=stage=='saved';inserted=stage in ('inserted','saved');known=case in ('known','hidden')
        query='' if stage=='opened' else 'alex'
        if case=='recovery' and stage in ('retry-member-discovery','retry-member-held','query-ready','inserted','saved'):query='al'
        if case=='retirement':
            if stage in ('query-replaced','late-settled'):query='project'
            if stage in ('replacement-member-discovery','replacement-member-held','cancelled'):query='al'
        if query and not seen:
            for method,value in [('users.prefs.get',S.policy(case)),('client.gantryBoot',gantry),('experiments.getByUser',dict(ok=True,assignments=dict(texty_autocomplete_files=dict(group='on'))))]:
                requests.append(dict(serial=len(requests)+1,method=method,args={},raw=copy.deepcopy(value)))
        if query and query not in seen:
            seen.append(query)
            families=['channels/search'] if query=='project' else ['users/search'] if query=='al' else ['users/search','usergroups/search','search.autocomplete.files']
            for method in families:
                args={'query':query}
                if method=='users/search':args.update(count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace=U.TEAM)
                if method=='usergroups/search':args.update(count=25,org_wide=True)
                if method=='channels/search':args.update(count=25,fuzz=1,filter='xws',include_record_channels=True,check_membership=True,top_channels=[],default_workspace=U.TEAM)
                if method=='search.autocomplete.files':args.update(include_shares='true',_x_reason='texty-autocomplete')
                requests.append(dict(serial=len(requests)+1,method=method,args=args,raw=raw if method=='search.autocomplete.files' else dict(ok=True,results=[])))
        if case=='recovery' and stage=='query-ready':
            requests.append(dict(serial=len(requests)+1,method='usergroups/search',args=dict(query='al',count=25,org_wide=True),raw=dict(ok=True,results=[])))
        if saved:requests.append(dict(serial=len(requests)+1,method='files.info',args=dict(file='FCOMPLETE1',page='1',count='500',truncate='true'),raw=dict(ok=True,file=files[0])))
        counts={m:sum(r['method']==m for r in requests) for m in S.FAMILIES};total=sum(counts.values())
        source=dict(requests=copy.deepcopy(requests),searches=total,completed=total,active=0,cancelled=0,held=False,heldQueries=[],networkCalls=0,
            channelSearches=counts['channels/search'],memberSearches=counts['users/search'],groupSearches=counts['usergroups/search'],fileSearches=counts['search.autocomplete.files'])
        model=dict(enabled=True,account=U.ACCOUNT,epoch=0,visit=2,serial=4 if saved else 3,busy=False,uncertain=False,dirty=False,
            managedValue='Preserved organization value',choiceValue='Morning',countryValue='US',blocks=copy.deepcopy(wire if saved else initial),reads=int(saved),attempts=[])
        if saved:model['attempts']=[[dict(element_id='long',long_text=dict(blocks=json.dumps(wire))),dict(element_id='locked',text=dict(text='Preserved organization value'))]]
        count=0 if known or stage=='opened' else 1 if stage=='member-discovery' else 3 if stage in ('retry-member-discovery','replacement-member-discovery') else 4 if query=='al' else 2
        pending={'member-discovery':1,'member-held':2,'query-replaced':2,'retry-member-discovery':3,'retry-member-held':4,
                 'replacement-member-discovery':3,'replacement-member-held':4}.get(stage)
        mr=member_requests(count,pending,case,retired);started=0 if not count else 1 if count<=2 else 2;active=int(pending is not None)
        present=known or (case=='recovery' and stage in ('query-ready','inserted','saved'))
        member=dict(profile=copy.deepcopy(model),account=U.ACCOUNT,epoch=0,sourcePresent=present,sourceObserved=True,native=U.member_raw() if present else None,
                    requests=mr,pending=[copy.deepcopy(r) for r in mr if not r['released']],driver=dict(started=started,finished=started-active,active=active))
        title=dict(case=case,seed=S.seed(case),fileResponseBytes=raw,policy=S.policy(case),member=member,networkCalls=0)
        native_blocks=copy.deepcopy(wire if inserted else initial)
        if query and not inserted:native_blocks[0]['elements'][0]['elements'].append(dict(type='text',text=' '+('#project' if query=='project' else '@'+query)))
        tokens=[v for v in native_blocks[0]['elements'][0]['elements'] if v['type']!='text']
        native=dict(snapshot=dict(blocks=native_blocks,generation='22',revision=1,composing=False),tokens=[dict(value=v,label=l) for v,l in zip(tokens,['@Alex Local','@Alex Local']+([U.label(case)] if inserted else []))])
        if stage=='cancelled':native=None;tokens=[]
        choices=None
        if query and not inserted and stage!='cancelled':
            seq=seen.index(query)+1
            values=[dict(value=dict(type='user',user_id='UMENTION01'),label='@Local real')]
            if stage=='query-ready':
                values=[dict(value=dict(type='user',user_id=id),label=label) for id,label in zip(
                    ['UMENTION01','UCOMPLETE1','UCOMPLETE2','UCOMPLETE3','UINIT0001'],['@Local real','@Profile real UCOMPLETE1','@Profile real UCOMPLETE2','@Profile real UCOMPLETE3','@Profile real UINIT0001'])]
                values += [dict(value=token,label=U.label(case)),dict(value=R['token']('FCOMPLETE2'),label='Alex List')]
            if query=='project':values=[dict(value=dict(type='channel',channel_id=id),label=label) for id,label in [('CCOMPLETE1','#project-completion'),('CCOMPLETE2','#project-cove')]]
            choices=dict(sequence=seq,revision=1,values=values)
            if stage=='query-ready':
                canonical=U.member_source();effective=dict(U.member_raw(),is_stranger=False)
                if known:canonical.pop('is_stranger');effective.pop('is_stranger')
                choices['titleUsers']={'FCOMPLETE1':dict(current=True,generation=f'profile-member-search-2-3-{seq}-file-title-users',
                    members=[dict(source=canonical,effective=effective)],policy=dict(user=S.policy(case),gantry=gantry,enterprise=None,revision=0))}
                ready_choices=copy.deepcopy(choices)
        controls=['Back','Save section']+([] if stage=='cancelled' else ['Use description','Cancel description edit'])
        g=dict(ok=True,noOverflow=True,viewport=viewport,dialog=rect(50,10,500,350),controls=[dict(text=c,disabled=c=='Save section',visible=True,rect=rect()) for c in controls],
            notices=[dict(text=R['FAILURE'],visible=True,rect=rect())] if stage=='unavailable' else [],rows=[],tokens=[dict(text=t['label'],visible=True,rect=rect()) for t in (native['tokens'] if native else [])],
            native=native,pngDimensions=[1002,600] if narrow else [1920,1200])
        if choices:
            g['resultRows']=[dict(id=f'option-{choices["sequence"]}-{i}',index=i,text=v['label'],visible=True,rect=rect(100,200+i*12,300,10)) for i,v in enumerate(choices['values'])]
            g['rows']=copy.deepcopy(g['resultRows']);g['popup']=dict(id='popup',sequence=choices['sequence'],rect=rect(90,190,320,100),scrollTop=0,clientHeight=100,scrollHeight=100,targetIndex=5 if stage=='query-ready' else None)
        exercise={}
        if stage=='opened':exercise=dict(initialBlocks=initial)
        elif stage in O.SERIALS:exercise=dict(serial=O.SERIALS[stage])
        elif stage=='unavailable':exercise=dict(failedMember=2)
        elif stage=='query-ready':exercise=dict(choices=copy.deepcopy(choices),native=dict(ok=True,sequence=choices['sequence'],serial='3',native=copy.deepcopy(native),
            rows=[dict(id=r['id'],label=r['text'],value=v['value']) for r,v in zip(g['resultRows'],choices['values'])]))
        elif stage=='inserted':exercise=dict(choice=dict(ok=True,id='FCOMPLETE1',type='canvas',keyboard=True,index=5,label=U.label(case),sequence=ready_choices['sequence'],values=ready_choices['values']),expectedBlocks=wire)
        elif stage=='saved':exercise=dict(expectedBlocks=wire,exactReadback=True)
        elif stage=='query-replaced':replacement=copy.deepcopy(choices);exercise=dict(heldMember=2,replacement=replacement)
        elif stage=='late-settled':exercise=dict(release=dict(serial=2,receiverRetired=retired,delivered=not retired),replacement=replacement)
        elif stage=='cancelled':exercise=dict(release=dict(serial=4,receiverRetired=retired,delivered=not retired),expectedBlocks=initial)
        source_files=dict(unavailableIds=[],excludedIds=['FCOMPLETE3'],rows=[dict(id=r['id'],source=r,effective={**r,'channels':channels}) for r,channels in zip(files[:2],[['CZMAP001','CAMAP001','CTAIL001'],['CYMAP002','CBMAP002']])]) if query else None
        result['stages'].append(dict(stage=stage,model=model,source=source,geometry=g,proof=dict(exercise=exercise,titleCompletion=title,choices=choices,sourceFiles=source_files)))
        result['captures'][stage]=str(PATH.resolve()/f'{fixture}-{stage}.png')
    last=result['stages'][-1];result.update(model=copy.deepcopy(last['model']),source=copy.deepcopy(last['source']),titleCompletion=copy.deepcopy(last['proof']['titleCompletion']))
    return fixture,result


class UserTitleCompletionTests(unittest.TestCase):
    def setUp(self):self.fixture,self.value=sample()
    def row(self,stage):return next(r for r in self.value['stages'] if r['stage']==stage)
    def validate(self):return O.validate(self.fixture,self.value,PATH,R,case=self.value['case'],platform='win32',png=False)
    def reject(self,mutation):
        mutation()
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):self.validate()
    def test_all_four_cases_both_widths(self):
        for case in STAGES:
            for narrow in (False,True):self.fixture,self.value=sample(case,narrow);self.validate()
    def test_shared_retirement_retains_actual_disposition(self):
        for retired in (False,True):self.fixture,self.value=sample('retirement',retired=retired);self.validate()
    def test_absent_member_not_seeded(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('opened')['proof']['titleCompletion']['seed']['boot']['users'].append(U.member_raw()))
    def test_original_raw_bytes_cannot_be_reserialized(self):
        self.reject(lambda:self.row('query-ready')['proof']['titleCompletion'].update(fileResponseBytes=json.dumps(json.loads(raw_bytes()),sort_keys=True)))
    def test_file_raw_nested_user_style_retained(self):
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['source']['title_blocks'][0]['elements'][0]['elements'][1].pop('style'))
    def test_effective_file_title_uses_original_blocks(self):
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].update(title_blocks=[]))
    def test_rank_input_remains_actual_plain_title(self):
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].update(title=U.label('known')))
    def test_channel_map_order_not_sorted(self):
        self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
    def test_query_subset_not_existing_token_cache(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1'].update(generation='profile-canvas-title-users'))
    def test_query_subset_sequence_owned(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1'].update(generation='profile-member-search-2-3-999-file-title-users'))
    def test_query_subset_current(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1'].update(current=False))
    def test_query_subset_original_member_not_native_only(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1']['members'][0].update(source=U.member_raw()))
    def test_query_subset_deduplicates_nested_user(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1']['members'].append({}))
    def test_query_policy_actual_reply(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1']['policy']['user'].update(opaque_policy='other'))
    def test_query_policy_actual_gantry(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['titleUsers']['FCOMPLETE1']['policy'].update(gantry={'ok':True}))
    def test_hidden_policy_required(self):
        self.fixture,self.value=sample('hidden')
        self.reject(lambda:self.row('query-ready')['proof']['titleCompletion']['policy']['prefs'].update(hidden_users=''))
    def test_realname_policy_cannot_override_user_title(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['values'][5].update(label='Review with @Profile fallback'))
    def test_private_label_cannot_replace_wire_url(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['values'][5]['value'].update(text=U.label('known')))
    def test_hosted_file_not_admitted(self):
        self.reject(lambda:self.row('query-ready')['proof']['choices']['values'][6]['value'].update(file_id='FCOMPLETE3'))
    def test_query_dom_sequence_matches_private_owner(self):
        self.reject(lambda:self.row('query-ready')['proof']['exercise']['native'].update(sequence=99))
    def test_query_dom_original_option_identity(self):
        self.reject(lambda:self.row('query-ready')['proof']['exercise']['native']['rows'][5].update(id='replacement'))
    def test_private_selection_original_identity(self):
        self.reject(lambda:self.row('inserted')['proof']['exercise']['choice'].update(id='FCOMPLETE2'))
    def test_private_selection_original_index(self):
        self.reject(lambda:self.row('inserted')['proof']['exercise']['choice'].update(index=4))
    def test_private_selection_original_query(self):
        self.reject(lambda:self.row('inserted')['proof']['exercise']['choice'].update(sequence=99))
    def test_original_tokens_preserved(self):
        self.reject(lambda:self.row('inserted')['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_unconsumed_query_not_saved(self):
        self.reject(lambda:self.row('inserted')['geometry']['native']['snapshot']['blocks'][0]['elements'][0]['elements'][-1].update(text='@alex'))
    def test_native_label_literal_entity_not_unescaped(self):
        self.reject(lambda:self.row('inserted')['geometry']['native']['tokens'][2].update(label=U.label('known').replace('&amp;','&')))
    def test_no_composition_capture(self):
        self.reject(lambda:self.row('query-ready')['geometry']['native']['snapshot'].update(composing=True))
    def test_exact_save_wire_not_display_label(self):
        self.reject(lambda:self.row('saved')['model']['attempts'][0][0]['long_text'].update(blocks='[]'))
    def test_original_siblings_retained(self):
        self.reject(lambda:self.row('inserted')['model'].update(choiceValue='Evening'))
    def test_original_section_not_replaced_by_query(self):
        self.reject(lambda:self.row('query-ready')['model'].update(serial=40))
    def test_reopen_requires_actual_files_info(self):
        self.reject(lambda:self.row('saved')['source']['requests'].pop())
    def test_raw_query_response_retained(self):
        self.reject(lambda:self.row('query-ready')['source']['requests'][-1].update(raw='{}'))
    def test_wrong_file_query_factory(self):
        self.reject(lambda:self.row('query-ready')['source']['requests'][-1]['args'].update(include_shares='false'))
    def test_recovery_discovery_before_member(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('retry-member-discovery')['proof']['titleCompletion']['member']['requests'][2].update(kind='member'))
    def query_family_mutation(self,case,stage,method,*,remove=False):
        _,receipt=sample(case)
        row=next(r for r in receipt['stages'] if r['stage']==stage)
        source=row['source'];requests=source['requests']
        if remove:
            requests[:]=[r for r in requests if (r['method'],r['args'].get('query'))!=(method,'al')]
        else:
            args=dict(query='al',count=25,org_wide=True)
            raw=dict(ok=True,results=[])
            if method=='search.autocomplete.files':
                args=dict(query='al',include_shares='true',_x_reason='texty-autocomplete');raw=raw_bytes()
            requests.append(dict(method=method,args=args,raw=raw))
        for n,r in enumerate(requests,1):r['serial']=n
        counts={m:sum(r['method']==m for r in requests) for m in S.FAMILIES}
        source.update(searches=sum(counts.values()),completed=sum(counts.values()),channelSearches=counts['channels/search'],
                      memberSearches=counts['users/search'],groupSearches=counts['usergroups/search'],fileSearches=counts['search.autocomplete.files'])
        with self.assertRaisesRegex(RuntimeError,'Completion repeated/omitted'):
            S.completion(source,case,stage,raw_bytes(),{})
    def test_short_query_cannot_dispatch_files(self):
        self.query_family_mutation('retirement','replacement-member-held','search.autocomplete.files')
    def test_retired_held_dependency_cannot_dispatch_groups(self):
        self.query_family_mutation('retirement','cancelled','usergroups/search')
    def test_retirement_still_requires_fresh_people_search(self):
        self.query_family_mutation('retirement','replacement-member-discovery','users/search',remove=True)
    def test_recovery_held_dependency_precedes_group_dispatch(self):
        self.query_family_mutation('recovery','retry-member-held','usergroups/search')
    def test_recovered_dependency_requires_group_dispatch(self):
        self.query_family_mutation('recovery','query-ready','usergroups/search',remove=True)
    def test_recovery_failed_reply_retained(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('query-ready')['proof']['titleCompletion']['member']['requests'][1].update(response={'raw':U.member_response()}))
    def test_recovery_no_premature_member_source(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('member-held')['proof']['titleCompletion']['member'].update(sourcePresent=True,native=U.member_raw()))
    def test_recovery_query_sequence_advances(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('retry-member-discovery')['proof']['choices'].update(sequence=1))
    def test_recovery_failure_notice_visible(self):
        self.fixture,self.value=sample('recovery')
        self.reject(lambda:self.row('unavailable')['geometry'].update(notices=[]))
    def test_retirement_no_late_member_adoption(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('late-settled')['proof']['titleCompletion']['member'].update(sourcePresent=True,native=U.member_raw()))
    def test_retirement_replacement_catalog_unchanged(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('late-settled')['proof']['choices'].update(revision=42))
    def test_retirement_no_old_disposition_rewrite(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('late-settled')['proof']['exercise']['release'].update(delivered=False))
    def test_cancel_retains_original_rich_draft(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('cancelled')['model'].update(blocks=[]))
    def test_cancel_cannot_leave_private_choices(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('cancelled')['proof'].update(choices=self.row('query-replaced')['proof']['choices']))
    def test_cancel_no_unrequested_write(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('cancelled')['model']['attempts'].append([]))
    def test_cancel_original_callback_must_settle(self):
        self.fixture,self.value=sample('retirement')
        self.reject(lambda:self.row('cancelled')['proof']['titleCompletion']['member']['driver'].update(active=1))
    def test_no_network(self):
        self.reject(lambda:self.row('query-ready')['source'].update(networkCalls=1))
    def test_missing_stages(self):self.reject(lambda:self.value['stages'].pop(1))
    def test_foreign_capture_path(self):self.reject(lambda:self.value['captures'].update(inserted=str(PATH/'foreign.png')))
    def test_clipped_popup_target(self):
        self.reject(lambda:self.row('query-ready')['geometry']['rows'][5]['rect'].update(right=900,width=800))
    def test_terminal_drift(self):self.reject(lambda:self.value['model'].update(reads=20))
    def test_baseline_runner_unchanged_and_isolated(self):
        base=RUN['harness']('known')
        self.assertEqual(base['STAGES'],tuple(STAGES['known']))
        self.assertEqual(base['PREFIX'],'profile-user-title-completion-check: ')
        self.assertEqual(base['EDGE'],'profile-channel-completion-capture: ')
        self.assertIs(base['validate_closed'].__globals__,base)
        self.assertEqual(R['B']['PREFIX'],'profile-file-completion-check: ')
    def test_unknown_case_refused(self):
        with self.assertRaises(RuntimeError):RUN['harness']('other')
    def test_exact_new_flags_no_launch(self):
        base=RUN['harness']('retirement');cls=base['NativeFixture'];original=cls.__bases__[0];captured=[]
        with patch.object(original,'__init__',lambda self,*args,**kwargs:captured.append((args,kwargs))):cls('profile-sections-mentions',Path('x'),Path('y'),{})
        self.assertEqual(captured[0][0][-1],{'SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CHECK':'1','SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CASE':'retirement'})


if __name__=='__main__':unittest.main()
