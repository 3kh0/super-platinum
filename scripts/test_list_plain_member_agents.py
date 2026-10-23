"""Portable exact catalog receipts and refusal/retry custody mutations."""
import copy,json,runpy,unittest
from pathlib import Path
import test_list_plain_member_save as M
import slack_list_plain_member_agents as V
import slack_list_plain_member_agents_facts as A
D=M.D

def sample(narrow=False):
    fixture,r,closed=M.sample(narrow);r['case']=V.CASES[0];rows=[r['stages'][i] for i in (0,1,3,5,6,7)];r['stages']=rows
    source=copy.deepcopy(rows[0]['plain']['memberSource']);agent=A.F['canonical'](A.AGENT)
    source['rows'].append(dict(source=agent,native=copy.deepcopy(A.AGENT)));source['retained']=copy.deepcopy(sorted(source['rows'],key=lambda r:r['source']['id']))
    raw=[pair['source'] for pair in source['rows']];owner={k:source[k] for k in ('account','epoch','team','user')}
    ledger=[]
    for i,row in enumerate(rows):
        p=row['plain'];p['memberSource']=copy.deepcopy(source)
        p['completion']['seed']['boot']['users'].append(copy.deepcopy(A.AGENT))
        for key in ('memberEvent','memberCallback','memberRelease'):row['proof'].pop(key,None)
        p['members'].update(requests=[],held=None,repair=None)
        count=0 if i<2 else 3 if i==2 else 6
        ledger=[]
        for j in range(count):
            method,args,response=A.READS[j%3];failed=j==2
            ledger.append(dict(serial=j+1,method=method,args=args,owner=owner,socket=source['socket'],response={'error':'offline'} if failed else response,disposition='offline' if failed else 'response',currentAtReturn=True))
        p['agentCatalog']=dict(enabled=True,requests=copy.deepcopy(ledger),noLiveNetwork=True)
        if i>=3:
            for j,item in enumerate(p['members']['lookups']):
                o=item['observation'];q=o['lookup']['query'];o['source']=copy.deepcopy(raw)
                opt=M.options(q,raw);opt['comparisons'][A.ID]='restricted-agent'
                for text in ('agent-raw-username','restricted-agent','Restricted Agent'):opt['fields'].append(dict(text=text,lowered=text.lower(),localeLowered=text.lower()))
                opt['classes']['restricted-agent']=5;o['options']=opt
                o['selectors']['members'].pop(A.MEMBER,None)
                if j==0:o['selectors']['members'][A.MEMBER]=A.F['selector_row'](A.F['canonical'](A.F['raw_member']()))
                o['results']=[A.F['canonical'](A.F['raw_member']())] if j==0 else []
                e=M.eligibility(raw)
                agentfact=next(x for x in e['rows'] if x['id']==A.ID);agentfact.update(agent=True,admittedAgent=False)
                e['agentAccess']=dict(mode='fetch',complete=True,assignments=copy.deepcopy(row['model']['fixture']['policy'][-2]['response']),reads=[dict(method='POST',raw=q['response']) for q in ledger[3:]])
                o['eligibility']=e
        if i>=2:
            row['proof']['failedSaveBefore']=copy.deepcopy(rows[1]['model']);row['proof']['failedSaveAfter']=copy.deepcopy(rows[2]['model'])
    r['proof']={**copy.deepcopy(rows[-1]['proof']),'closedDisk':r['proof']['closedDisk']};r['final']=copy.deepcopy(rows[-1]['model'])
    return copy.deepcopy((fixture,r,closed))

class AgentTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,png=False)
    def test_full_both_widths_and_closed_database(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_ready_source_encounter_order(self):self.reject(lambda r:r['stages'][0]['plain']['memberSource']['rows'].reverse())
    def test_retained_source_id_order(self):self.reject(lambda r:r['stages'][0]['plain']['memberSource']['retained'].reverse())
    def test_retained_source_pair_not_foreign(self):self.reject(lambda r:r['stages'][0]['plain']['memberSource']['retained'][0]['native'].update(name='foreign'))
    def test_actual_catalog_failure(self):self.reject(lambda r:r['stages'][2]['plain']['agentCatalog']['requests'][2].update(response=A.CATALOG,disposition='response'))
    def test_no_failure_default_off(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'].append({}))
    def test_failed_save_no_attempt(self):self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'].append({}))
    def test_failed_source_still_current(self):self.reject(lambda r:r['stages'][2]['plain']['memberSource'].update(current=False))
    def test_no_member_repair_substitute(self):self.reject(lambda r:r['stages'][2]['plain']['members']['requests'].append({}))
    def test_agent_inventory_required(self):self.reject(lambda r:r['stages'][0]['plain']['memberSource']['rows'].pop())
    def test_original_agent_raw(self):self.reject(lambda r:r['stages'][3]['plain']['memberSource']['rows'][2]['source'].update(is_agentforce_bot=False))
    def test_actual_settings_disable(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][4]['response']['team_prefs']['toplevel_prefs']['ai_apps_settings']['apps'][0].update(is_ai_enabled=True))
    def test_original_agent_profile(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5]['response']['apps'][0]['bot_user'].update(id='UFOREIGN1'))
    def test_complete_list_not_partial(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5]['response']['response_metadata'].update(next_cursor='more'))
    def test_original_factory(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5]['args'].update(limit='100'))
    def test_original_context(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5]['owner'].update(epoch=99))
    def test_original_socket(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5].update(socket=99))
    def test_original_currentness(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'][5].update(currentAtReturn=False))
    def test_no_unrequested_profile_read(self):self.reject(lambda r:r['stages'][3]['plain']['agentCatalog']['requests'].append({}))
    def test_no_second_query_catalog(self):self.reject(lambda r:r['stages'][4]['plain']['agentCatalog']['requests'].extend(copy.deepcopy(r['stages'][4]['plain']['agentCatalog']['requests'][3:])))
    def test_actual_agent_denied(self):self.reject(lambda r:next(x for x in r['stages'][3]['plain']['members']['lookups'][0]['observation']['eligibility']['rows'] if x['id']==A.ID).update(admittedAgent=True))
    def test_missing_agent_is_not_false(self):self.reject(lambda r:next(x for x in r['stages'][3]['plain']['members']['lookups'][0]['observation']['eligibility']['rows'] if x['id']==A.ID).update(admittedAgent=None))
    def test_receipt_no_partial_grant(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][0]['observation']['eligibility']['agentAccess'].update(complete=False))
    def test_receipt_not_failed_read(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][0]['observation']['eligibility']['agentAccess']['reads'][2].update(raw={'error':'offline'}))
    def test_receipt_no_default_flags(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][0]['observation']['eligibility']['agentAccess'].update(assignments={}))
    def test_observed_no_match(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][1]['observation'].update(results=[A.F['canonical'](A.AGENT)]))
    def test_ordinary_human_identity(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][0]['observation']['results'][0].update(id=A.ID))
    def test_renderer_original_inventory(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'][0]['observation']['options']['originalMembers'].reverse())
    def test_independent_reply(self):self.reject(lambda r:r['stages'][2]['journal'].update(blocks=[]))
    def test_same_editor(self):self.reject(lambda r:r['stages'][2]['geometry']['edit'].update(sameNode=False))
    def test_original_failed_delta(self):self.reject(lambda r:r['stages'][2]['editInventory']['plain_delta'].update(ops=[]))
    def test_only_one_changed_attempt(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_original_runner(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-plain-member-agents-native.py')))['harness']()
        self.assertEqual(h['case_contract'](V.CASES[0])[0],V.STAGES[V.CASES[0]])

if __name__=='__main__':unittest.main()
