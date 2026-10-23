"""Portable mixed-candidate lifecycle examples; never invokes native or Slack."""
import copy,json,runpy,unittest
from pathlib import Path
import slack_list_plain_mixed_save as V
import slack_list_plain_mixed_save_facts as F
import test_list_plain_member_save as M
D=M.D
TIME=1792771200123

def sample(case=V.CASES[0],narrow=False):
    wire=F.history_wire(case,TIME);v=V.configured(case,wire)
    def transform(x):
        if isinstance(x,(list,tuple)):return [transform(y) for y in x]
        if isinstance(x,dict):
            if x==M.V.BODY[0]:return copy.deepcopy(v['BODY'][0])
            return {k:transform(y) for k,y in x.items()}
        if isinstance(x,str):
            return x.replace(M.V.FUTURE_BYTES,v['FUTURE_BYTES']).replace(M.V.WIRE_TEXT,v['WIRE_TEXT']).replace('own-edit-plain-member',case).replace(json.dumps(M.V.BODY[0],separators=(',',':')),json.dumps(v['BODY'][0],separators=(',',':')))
        return x
    fixture,r,closed=transform(M.sample(narrow));old=r['stages'];rows=[old[i] for i in (0,1,5,6,7)];r['stages']=rows
    original=copy.deepcopy(rows[0]['plain']['memberSource']);original['groups']=dict(rows=F.GROUPS,membership=[],current=True)
    source=[x['source'] for x in original['rows']]
    for i,row in enumerate(rows):
        p=row['plain'];proof=row['proof'];count=v['READS'][case][i]
        p['memberSource']=copy.deepcopy(original)
        p['mixed']=dict(enabled=True,case=case,capturedAtMillis=TIME,historyWire=wire,clock=F.CLOCK,noLiveNetwork=True)
        p['preferences']=copy.deepcopy(v['PREFS']);p['completion']['seed']['boot']['prefs']=copy.deepcopy(v['PREFS']['prefs'])
        p['canonical']=dict(locale='en-US',membership=[],rows=copy.deepcopy(F.GROUPS),current=True)
        p['completion']['groups']=dict(rows=copy.deepcopy(F.GROUPS),membership=[],selfRestricted=False)
        p['completion']['seed']['groups']=dict(incoming=[F.GROUP],rows=F.GROUPS,membership=[])
        ledger=p['members'];ledger.update(requests=[],held=None,repair=None)
        for serial,item in enumerate(ledger['lookups'],1):
            o=item['observation'];q='harbor' if serial==1 else 'not-found';opt=M.options(q,source);opt['groups']=F.GROUPS
            texts={'',q,'harbor','Harbor Person','harbor@fixture.invalid','fixture-self','username-must-not-win','Alex group SLISTGRP1','alex group slistgrp1','@harbor'}
            opt['fields']=[dict(text=t,lowered=t.lower(),localeLowered=t.lower()) for t in sorted(texts)]
            opt['classes']={n:i for i,n in enumerate(['alex group slistgrp1','Alex group SLISTGRP1','fixture-self','harbor'])}
            selectors=M.selectors(serial==1)
            if serial==1:selectors['members'][F.MEMBER]=F.M.selector_row(F.M.canonical(F.M.raw_member()))
            result=[F.M.canonical(F.M.raw_member()),F.EFFECTIVE] if serial==1 else []
            if serial==1 and case.endswith('-group'):result.reverse()
            o.update(source=source,groups=F.GROUPS,options=opt,selectors=selectors,eligibility=M.eligibility(source),results=result)
        for k in ('memberEvent','memberRelease','memberCallback','failedSaveBefore','failedSaveAfter','failedSave'):proof.pop(k,None)
        model=row['model'];c=model['comments'];l=c['ledger']
        if i>=2:
            l['requests']=l['requests'][:8]+l['requests'][12:]
            for serial,q in enumerate(l['requests'],1):q['serial']=serial
            l['completed']=count;l['replies']=count//4
            ordered=row['ordered'];ordered['requests'].pop(2);ordered['pages'].pop(2);ordered['completed']=count//4
            for n,(q,page) in enumerate(zip(ordered['requests'],ordered['pages']),1):q['serial']=4*n;page['requestSerial']=4*n
        model['fixture']['policy']=[dict(request=q,response=raw) for _ in range(v['POLICIES'][case][i]) for q,raw in v['policy_cycle']()]
    r['proof']={**copy.deepcopy(rows[-1]['proof']),'closedDisk':r['proof']['closedDisk']};r['final']=copy.deepcopy(rows[-1]['model'])
    return copy.deepcopy((fixture,r,closed))

class MixedTests(unittest.TestCase):
    def reject(self,change,case=V.CASES[0]):
        f,r,_=sample(case);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,png=False)
    def test_both_variants_both_widths_and_closed(self):
        for case in V.CASES:
            for narrow in (False,True):
                f,r,c=sample(case,narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_authored_future_bytes(self):
        for case in V.CASES:
            v=V.configured(case,F.history_wire(case,TIME));p=Path(__file__).parents[1]/('src/desktop/slack_lists/comments/fixture/plain/mixed-'+('group' if case.endswith('-group') else 'tie')+'-future.json')
            self.assertEqual(p.read_bytes().decode('utf-8'),v['FUTURE_BYTES'])
    def test_exact_closed_runner(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-plain-mixed-save-native.py')))['harness']()
        for case in V.CASES:self.assertEqual(h['case_contract'](case)[0],V.STAGES[case])
    def test_history_must_be_string(self):self.reject(lambda r:r['stages'][0]['plain']['mixed'].update(historyWire={}))
    def test_history_not_seconds(self):self.reject(lambda r:r['stages'][0]['plain']['mixed'].update(capturedAtMillis=TIME//1000))
    def test_history_same_actual_visit(self):self.reject(lambda r:r['stages'][0]['plain']['mixed'].update(historyWire=F.history_wire(V.CASES[0],TIME+1)))
    def test_tie_cannot_have_winner_counts(self):self.reject(lambda r:r['stages'][0]['plain']['mixed'].update(historyWire=F.history_wire(V.CASES[1],TIME)))
    def test_fresh_policy_not_empty(self):self.reject(lambda r:r['stages'][2]['model']['fixture']['policy'][5]['response']['prefs'].update(frecency='{}'))
    def test_original_boot_history(self):self.reject(lambda r:r['stages'][0]['plain']['completion']['seed']['boot']['prefs'].pop('frecency'))
    def test_group_handle_collision_required(self):self.reject(lambda r:r['stages'][0]['plain']['canonical']['rows'][0]['effective'].update(handle='alex-team'))
    def test_both_candidates_required(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][0]['observation']['results'].pop())
    def test_effective_group_marker_required(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][0]['observation']['results'][1].pop('is_usergroup'))
    def test_no_username_substitution(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][0]['observation']['results'][0].update(name='harbor'))
    def test_observed_unknown_empty(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][1]['observation'].update(results=[F.EFFECTIVE]))
    def test_lookup_owner_current(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][0]['observation'].update(groupsCurrent=False))
    def test_original_rank(self):self.reject(lambda r:r['stages'][2]['plain']['members']['lookups'][0]['observation'].update(rankCurrent=False))
    def test_no_source_reseed(self):self.reject(lambda r:r['stages'][2]['plain']['memberSource'].update(socket=9))
    def test_no_member_repair(self):self.reject(lambda r:r['stages'][2]['plain']['members']['requests'].append({}))
    def test_tie_literal_not_semantic(self):self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'].__setitem__(0,{'type':'usergroup','usergroup_id':'SLISTGRP1'}))
    def test_decisive_correct_group(self):self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(usergroup_id='SFOREIGN1'),V.CASES[1])
    def test_reply_journal_unchanged(self):self.reject(lambda r:r['stages'][2]['journal'].update(blocks=[]))
    def test_native_same_editor(self):self.reject(lambda r:r['stages'][1]['geometry']['edit'].update(sameNode=False))
    def test_held_duplicate_disabled(self):self.reject(lambda r:r['stages'][2]['proof']['doubleSave'].update(disabled=False))
    def test_context_suffix_unchanged(self):self.reject(lambda r:r['stages'][2]['model']['edit']['attempts'][0]['blocks'].pop())
    def test_closed_history(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)

if __name__=='__main__':unittest.main()
