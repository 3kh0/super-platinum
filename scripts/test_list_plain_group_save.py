"""Portable group-source and semantic Save counterexamples; no native launch."""
import copy,json,runpy,unittest
from pathlib import Path
import slack_list_plain_group_save as V
import slack_list_plain_group_save_facts as F
import test_list_plain_member_save as M
D=M.D

def transform(v):
    if isinstance(v,(tuple,list)):return [transform(x) for x in v]
    if isinstance(v,dict):
        if v=={'type':'user','user_id':'ULISTMEM1'}:return {'type':'usergroup','usergroup_id':'SLISTGRP1'}
        return {k:transform(x) for k,x in v.items()}
    if isinstance(v,str):
        return v.replace('Known @harbor and @not-found','Known @alex-team and @not-found').replace('Known @ULISTMEM1 and @not-found','Known @SLISTGRP1 and @not-found').replace('own-edit-plain-member','own-edit-plain-group').replace('"type":"user","user_id":"ULISTMEM1"','"type":"usergroup","usergroup_id":"SLISTGRP1"')
    return v

def sample(narrow=False):
    fixture,r,closed=transform(M.sample(narrow));rows=r['stages'];owner=rows[0]['model']['comments'];base=copy.deepcopy(rows[0]['plain']['memberSource']);event=None;release=None
    for i,row in enumerate(rows):
        p=row['plain'];proof=row['proof'];pending=i in (2,3);updated=102 if i>=4 else 101;g=F.groups(updated)
        group=dict(account=owner['account'],epoch=owner['epoch'],team='T1',user=F.SELF,socket=base['socket'],pending=pending,current=not pending,rows=None if pending else copy.deepcopy(g),membership=None if pending else [],native=F.native(updated))
        p['groupSource']=group
        p['memberSource']=copy.deepcopy(base);p['memberSource']['groups']=None if pending else dict(rows=copy.deepcopy(g),membership=[],current=True)
        p['canonical']=None if pending else dict(locale='en-US',membership=[],rows=copy.deepcopy(g),current=True)
        p['completion']['groups']={'unavailable':True} if pending else dict(rows=copy.deepcopy(g),membership=[],selfRestricted=False)
        ledger=p['members'];ledger.update(requests=[],held=None,repair=None)
        source=[r['source'] for r in base['rows']]
        for serial,entry in enumerate(ledger['lookups'],1):
            o=entry['observation'];q='alex-team' if serial==1 else 'not-found'
            options=M.options(q,source);options['groups']=copy.deepcopy(g)
            o.update(lookup=dict(kind='member',query=q),source=source,groups=copy.deepcopy(g),options=options,eligibility=M.eligibility(source),selectors=M.selectors(False),results=[g[0]['effective']] if serial==1 else [])
            o['requested']['query']=q
        for k in ('memberEvent','memberRelease','memberCallback'):proof.pop(k,None)
        if i==2:event=dict(raw=dict(type='subteam_updated',subteam=dict(F.GROUP,date_update=102)),before=copy.deepcopy(rows[1]['plain']['groupSource']),source=copy.deepcopy(group),originalCurrent=False,contextCurrent=True)
        if i>=2:proof['groupEvent']=copy.deepcopy(event)
        if i==4:release=dict(source=copy.deepcopy(group),originalCurrent=False,contextCurrent=True,pairedCommit=True)
        if i>=4:proof['groupRelease']=copy.deepcopy(release)
    r['proof']={**copy.deepcopy(rows[-1]['proof']),'closedDisk':r['proof']['closedDisk']};r['final']=copy.deepcopy(rows[-1]['model'])
    return copy.deepcopy((fixture,r,closed))

class GroupTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,png=False)
    def test_full_both_widths_closed(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_original_future_bytes(self):
        p=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture/plain/group-future.json'
        self.assertEqual(p.read_bytes().decode('utf-8'),V.FUTURE_BYTES)
    def test_original_closed_runner(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-plain-group-save-native.py')))['harness']()
        self.assertEqual(h['case_contract'](V.CASES[0])[0],V.STAGES[V.CASES[0]])
    def test_pending_not_ready(self):self.reject(lambda r:r['stages'][3]['plain']['groupSource'].update(current=True))
    def test_pending_not_empty(self):self.reject(lambda r:r['stages'][3]['plain']['groupSource'].update(rows=[]))
    def test_pending_native_not_advanced(self):self.reject(lambda r:r['stages'][2]['plain']['groupSource']['native']['label_source'].update(date_update=102))
    def test_pair_must_commit(self):self.reject(lambda r:r['stages'][4]['plain']['groupSource']['native']['label_source'].update(date_update=101))
    def test_native_members_loaded_actual_event(self):self.reject(lambda r:r['stages'][4]['plain']['groupSource']['native'].update(members_loaded=False))
    def test_old_snapshot_never_resurrects(self):self.reject(lambda r:r['stages'][4]['proof']['groupRelease'].update(originalCurrent=True))
    def test_same_socket(self):self.reject(lambda r:r['stages'][4]['plain']['groupSource'].update(socket=9))
    def test_original_event(self):self.reject(lambda r:r['stages'][2]['proof']['groupEvent']['raw']['subteam'].update(date_update=103))
    def test_no_boot_reseed(self):self.reject(lambda r:r['stages'][4]['plain']['completion']['seed']['groups']['incoming'][0].update(date_update=102))
    def test_no_member_recovery(self):self.reject(lambda r:r['stages'][3]['plain']['members']['requests'].append({}))
    def test_independent_member_current(self):self.reject(lambda r:r['stages'][3]['plain']['memberSource'].update(current=False))
    def test_no_premature_lookup(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'].append({}))
    def test_no_premature_rank(self):self.reject(lambda r:r['stages'][3]['plain']['completion']['requests'].append({}))
    def test_group_current(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(groupsCurrent=False))
    def test_source_current(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(sourceCurrent=False))
    def test_rank_current(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(rankCurrent=False))
    def test_original_raw_order(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['source'].reverse())
    def test_effective_not_raw_result(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(results=[F.groups(102)[0]['source']]))
    def test_observed_unknown_not_group(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][1]['observation'].update(results=[F.groups(102)[0]['effective']]))
    def test_category_options(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['requested']['options']['entities']['userGroups'].update(includeFromAnotherTeam=False))
    def test_name_policy_membership(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['namePolicy'].update(defaultWorkspace='TFOREIGN'))
    def test_rank_usergroup_ctr(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['selectors']['entityCtr'].update(userGroup=0))
    def test_failed_no_attempt(self):self.reject(lambda r:r['stages'][3]['model']['edit']['attempts'].append({}))
    def test_failed_same_node(self):self.reject(lambda r:r['stages'][3]['geometry']['edit'].update(sameNode=False))
    def test_preserved_rich_reply(self):self.reject(lambda r:r['stages'][3]['journal'].update(blocks=[]))
    def test_preserved_delta(self):self.reject(lambda r:r['stages'][3]['editInventory']['plain_delta'].update(ops=[]))
    def test_semantic_wire_not_text(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'].__setitem__(1,{'type':'text','text':'@alex-team'}))
    def test_semantic_wire_id(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(usergroup_id='SFOREIGN1'))
    def test_context_suffix(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'].pop())
    def test_closed_history(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)

if __name__=='__main__':unittest.main()
