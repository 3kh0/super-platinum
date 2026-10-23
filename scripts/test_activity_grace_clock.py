"""Portable narrow calendar/receipt regressions; not native acceptance."""
import unittest
import activity_grace_clock as O


class CalendarContract(unittest.TestCase):
    def test_five_calendar_days_cross_dst(self):
        self.assertEqual(O.deadline(False)-O.START_MS,120*3600*1000)
        self.assertEqual(O.deadline(True)-O.START_MS,119*3600*1000)
        self.assertTrue(O.policy('CGRARCH01',O.BETWEEN_MS,False)['inGrace'])
        self.assertFalse(O.policy('CGRARCH01',O.BETWEEN_MS,True)['inGrace'])

    def test_exact_deadline_is_expired(self):
        for eastern in (False,True):
            end=O.deadline(eastern)
            self.assertTrue(O.policy('CGRARCH01',end-1,eastern)['inGrace'])
            for now in (end,end+1):
                value=O.policy('CGRARCH01',now,eastern)
                self.assertFalse(value['inGrace'])
                self.assertEqual(value['text'],'grace-archived (archived)')
                self.assertEqual(value['iconKind'],'Archive')

    def test_private_and_archived_decisions_remain_independent(self):
        before=O.policy('CGRPRIV01',O.START_MS,True)
        self.assertEqual(before['iconKind'],'LockFilled')
        self.assertFalse(before['archivedSuffix'])
        after=O.policy('CGRPRIV01',O.EASTERN_END_MS,True)
        self.assertEqual(after['iconKind'],'ArchiveFilled')
        self.assertFalse(after['archivedSuffix'])
        missing=O.policy('CGRNOGT01',O.START_MS,True)
        self.assertIsNone(missing['graceEndMs'])
        self.assertFalse(missing['inGrace'])

    def test_no_unknown_zone_or_timestamp_default(self):
        for value in (None,0,'UTC'):
            with self.assertRaises(RuntimeError):O.deadline(value)
        for value in (None,True,'1773072000000'):
            with self.assertRaises(RuntimeError):O.policy('CGRARCH01',value,True)
        with self.assertRaises(RuntimeError):O.policy('foreign',O.START_MS,True)




# Complete synthetic protocol vectors exercise the oracle, never native acceptance.
import copy
from pathlib import Path
import runpy
from unittest.mock import patch
import test_workflow_activity_native as Base


def sample(narrow=False):
    fixture, old = Base.channel_sample(narrow, spec=O.GRACE)
    rows=[]; records=[]; latest=None; retained={}
    for i,name in enumerate(O.STAGES):
        row=copy.deepcopy(old['stages'][min(i,2)])
        row['stage']=name
        m=row['model'];m['metadata'].update(title=Base.R['TITLE'],description=Base.R['DESCRIPTION'],dirty=i>0)
        m['metadata']['reads']=0 if i==0 else 12 if i==1 else 16
        m['activity']=dict(active=2<=i<9,detail={'workflow_event':{'workflow_execution_id':'E_REVIEW'}} if i>=2 else None,
                           detailRow={'workflow_execution_id':'E_REVIEW'} if i>=2 else None)
        m['export']=dict(attempts=[],retained=0,current=dict(shown=False,disabled=i>=9,notice=None))
        ch=row['proof']['channel'];seed=ch['seed'];seed['startSeconds']=O.START_MS//1000
        if i>=2:ch['documents'][0]['revision']+=1
        m['activity']['documentRevision']=ch['documents'][0]['revision'] if i else None
        seed['extras']['plan_info']['reconnect_grace_period_start_ts']=O.START_MS//1000
        for raw in seed['boot']['channels'][:5]:raw['unlinked']=O.START_MS//1000
        if i==1 or i>=9:
            row['geometry']['fields']=dict(title=Base.R['TITLE'],description=Base.R['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False)
        if i>=2:
            requests=m['ledger']['requests']
            requests.append(dict(serial=2,method='workflows.activity.get',args=dict(workflow_id='WF_REVIEW',workflow_execution_id='E_REVIEW',_x_reason='Load instance steps for activity tab')))
            m['ledger']['finished']=2
            count=O.CHANGE_COUNTS[i]
            change=O.CHANGES[count-1] if count else dict(nowMs=O.UTC_END_MS-1,eastern=False)
            now,east=change['nowMs'],change['eastern']
            if i in (2,3,4,5,7,8):
                rec=dict(id=len(records),generation=i,sample=dict(now_ms=now,eastern=east),
                         held=i in (5,8),finished=i not in (5,8),applied=i not in (5,8))
                rec['policies']=[]
                for id in O.IDS:
                    p=O.policy(id,now,east)
                    rec['policies'].append(dict(id=id,nowMs=now,**{k:p[k] for k in ('graceEndMs','inGrace','archiveIcon','archivedSuffix')}))
                if i==7:records[3].update(finished=True,applied=False)
                records.append(rec)
                if rec['applied']:
                    latest=rec
                    for p in rec['policies']:
                        if p['id'] not in retained or retained[p['id']]['graceEndMs']!=p['graceEndMs']:
                            retained[p['id']]=p
            if i==10:records[-1].update(finished=True,applied=False)
            labels=dict(current=True,clockGeneration=i,observedGeneration=latest['generation'],plan=copy.deepcopy(seed['extras']['plan_info']),rows=[])
            for raw in sorted(seed['boot']['channels'][:5],key=lambda r:r['id']):
                p=retained[raw['id']]
                labels['rows'].append(dict(id=raw['id'],raw=dict(raw,fromAnotherTeam=False,is_member=raw['id']!='CGRARCH01'),policy=dict(private=raw['id'] in O.GRACE.private_ids,capturedNowMs=p['nowMs'],**{k:p[k] for k in ('graceEndMs','inGrace','archiveIcon','archivedSuffix')})))
            if i>=9:labels={'current':False}
            clock=dict(sample=dict(now_ms=now,eastern=east),changes=list(O.CHANGES[:count]),projections=copy.deepcopy(records),held=i in (5,6,8,9),labels=labels,startSeconds=O.START_MS//1000,utcEnd=O.UTC_END_MS,easternEnd=O.EASTERN_END_MS)
            if i<9:
                spec=O.LabelSpec(i in (2,5,6));native=Base.channel_labels_sample(spec=spec)
                native['identity']=dict(ok=True,sameRoot=True,sameDetail=True,rows=[dict(id=node,channel=id,sameButton=True,sameName=True,connected=True,text=text) for node,id,text,_ in spec.vectors()])
            else:
                native=dict(activityPresent=False,detailPresent=False,originalDetailConnected=False,originalRootConnected=True,rows=[dict(id=node,connected=False,nameConnected=False) for node,_,_,_ in O.GRACE.vectors()])
            evidence=dict(clock=clock,native=native)
            if i==10:evidence['quiet']=dict(elapsedMillis=1251,before=copy.deepcopy(records),after=copy.deepcopy(records))
            row['proof']['graceClock' if i==2 else 'exercise']=evidence
        rows.append(row)
    old.update(graceClock=True,stages=rows,captures={name:str(Base.DIRECTORY/f'{fixture}-{name}.png') for name in O.STAGES},final=copy.deepcopy(rows[-1]['model']),clock=copy.deepcopy(clock))
    return fixture,old


class ReceiptContract(unittest.TestCase):
    def validate(self, fixture, value):
        return O.validate(fixture,value,Base.DIRECTORY,Base.R,platform='win32',png=False)

    def reject(self,index,mutate):
        f,v=sample();mutate(v['stages'][index]);
        with self.assertRaises((RuntimeError,KeyError,TypeError)):self.validate(f,v)

    def test_complete_two_width_vectors(self):
        for narrow in (False,True):
            f,v=sample(narrow);self.validate(f,v)

    def test_generation_advances_with_original_capture_and_expired_dom(self):
        f,v=sample();self.validate(f,v)
        first=v['stages'][2]['proof']['graceClock']['clock']['labels']
        expired=v['stages'][3]['proof']['exercise']
        self.assertGreater(expired['clock']['labels']['observedGeneration'],first['observedGeneration'])
        self.assertEqual(expired['clock']['labels']['rows'],first['rows'])
        self.assertTrue(first['rows'][0]['policy']['inGrace'])
        self.assertEqual(expired['native']['rows'][0]['iconKind'],'Archive')

    def test_clock_only_draft_write_refused(self):
        self.reject(4,lambda r:r['model']['metadata'].update(writes=1))

    def test_entry_policy_reads_are_exact_and_clock_never_reacquires(self):
        f,v=sample();self.validate(f,v)
        self.assertEqual([r['model']['metadata']['reads'] for r in v['stages']],[0,12]+[16]*9)
        self.reject(2,lambda r:r['model']['metadata'].update(reads=12))
        self.reject(4,lambda r:r['model']['metadata'].update(reads=20))

    def test_entry_revision_is_not_a_clock_write_allowance(self):
        self.reject(4,lambda r:r['proof']['channel']['documents'][0].update(revision=99))
        self.reject(2,lambda r:r['model']['activity'].update(documentRevision=99))

    def test_clock_only_model_mutation_refused(self):
        self.reject(4,lambda r:r['model'].update(epoch=4))

    def test_original_node_replacement_refused(self):
        self.reject(4,lambda r:r['proof']['exercise']['native']['identity'].update(sameRoot=False))

    def test_equal_deadline_old_label_refused(self):
        self.reject(3,lambda r:r['proof']['exercise']['native']['rows'][0].update(iconKind='Tag'))

    def test_stale_completed_projection_refused(self):
        self.reject(7,lambda r:r['proof']['exercise']['clock']['projections'][3].update(applied=True))

    def test_missing_generation_invalidation_refused(self):
        self.reject(6,lambda r:r['proof']['exercise']['clock']['labels'].update(clockGeneration=5))

    def test_original_computation_mutation_refused(self):
        self.reject(6,lambda r:r['proof']['exercise']['clock']['projections'][3]['policies'][0].update(graceEndMs=O.EASTERN_END_MS))

    def test_retired_projection_adoption_refused(self):
        self.reject(10,lambda r:r['proof']['exercise']['clock']['projections'][-1].update(applied=True))

    def test_post_leave_work_refused(self):
        self.reject(10,lambda r:r['proof']['exercise']['quiet']['after'].append({}))

    def test_short_quiet_refused(self):
        self.reject(10,lambda r:r['proof']['exercise']['quiet'].update(elapsedMillis=1249))

    def test_no_visible_draft_substitute(self):
        self.reject(9,lambda r:r['geometry']['fields'].update(description='lost'))

    def test_extra_network_read_refused(self):
        self.reject(4,lambda r:r['proof']['channel']['ledger']['requests'].append({}))

    def test_missing_retired_node_proof_refused(self):
        self.reject(9,lambda r:r['proof']['exercise']['native']['rows'][0].update(connected=True))

    def test_retirement_only_disables_existing_export_without_dispatch(self):
        self.reject(9,lambda r:r['model']['export']['current'].update(disabled=False))
        self.reject(9,lambda r:r['model']['export']['attempts'].append({'new':'dispatch'}))

    def test_sidebar_native_mismatch_refused(self):
        self.reject(4,lambda r:r['proof']['sidebarNative'].update(activeId='foreign'))

    def test_wrapper_exact_flag_only(self):
        wrapper=runpy.run_path(str(Path(__file__).with_name('check-activity-grace-clock-native.py')))
        with patch('native_fixture.NativeFixture') as native:
            module=wrapper['harness']()
            checks={module.FLAG:'1','SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE':O.CASE}
            module.NativeFixture(O.FIXTURES[0],'log','binary',checks)
            self.assertEqual(native.call_args.args[3],dict(checks,**{O.FLAG:'1'}))
            for case in ({},dict(checks,**{O.FLAG:'1'}),dict(checks,SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE='browse')):
                with self.assertRaises(RuntimeError):module.NativeFixture(O.FIXTURES[0],'log','binary',case)
            with self.assertRaises(RuntimeError):module.NativeFixture('foreign','log','binary',checks)


    def test_unknown_projection_sample_refused(self):
        self.reject(5,lambda r:r['proof']['exercise']['clock']['projections'][-1]['sample'].update(now_ms=1))

    def test_canonical_source_not_replaced_by_policy(self):
        self.reject(7,lambda r:r['proof']['exercise']['clock']['labels']['rows'][0]['raw'].update(fromAnotherTeam=True))

    def test_public_archived_canonical_membership_is_not_raw_boot_membership(self):
        self.reject(2,lambda r:next(x for x in r['proof']['graceClock']['clock']['labels']['rows'] if x['id']=='CGRARCH01')['raw'].update(is_member=True))

    def test_original_finished_history_immutable(self):
        f,v=sample()
        first=v['stages'][2]['proof']['graceClock']['clock']
        next_clock=copy.deepcopy(first)
        next_clock['projections'][0]['applied']=False
        with self.assertRaises(RuntimeError):O.projections(next_clock,first)


if __name__=='__main__':unittest.main()
