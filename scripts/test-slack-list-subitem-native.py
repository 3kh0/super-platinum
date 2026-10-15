"""Pure closed-evidence oracle regressions; never launches an app or reads Slack."""
import copy
import runpy
from pathlib import Path
import unittest
M=runpy.run_path(str(Path(__file__).with_name('check-slack-list-subitem-native.py')))
def receipt(fixture='slack-list-subitem-create'):
    uncertain='-uncertain' in fixture;timestamp='1000-0000-0123456789abcdef';current=M['current_population']()
    raw=M['expected_responses'](fixture);stages=['Preparing']*5+['Sending']+['Checking']*(6 if uncertain else 5)
    admissions=[]
    for n,stage in enumerate(stages):
        after=uncertain and n>=7
        admissions.append(dict(serial=5,job=6 if n<6 else 7 if not after else 8,stage=stage,list='F_LIST_FIXTURE',parent='R_HYDRATE_PARENT',visit=20 if after else 10,epoch=2 if after else 1,detail=22 if after else 12,selection=23 if after else 13))
    reply=None if uncertain else raw[5]
    return dict(fixture=fixture,before=M['create_document'](fixture),after=M['current_document'](fixture),initialPopulation=M['population'](),finalPopulation=current,selectedView='V_CUSTOM',layoutKind='board' if fixture.endswith('-narrow') else 'table',
        retainedReceipt=dict(serial=5,timestamp=timestamp,reply=reply,child=None if uncertain else M['server_child'](),stage='Uncertain' if uncertain else 'AwaitingRead'),retainedMutationReply=reply,positiveReplyRetained=not uncertain,explicitOpen=not uncertain,unknownIDNotGuessed=uncertain,lateReadRejected=uncertain,
        writeRequests=1,readRequests=11 if uncertain else 10,cellWriteRequests=0,fileViewingRequests=0,recordOpenPrepared=2 if uncertain else 3,visitGenerations=[10,20] if uncertain else [],
        ledger=dict(requests=M['expected_wire'](fixture,timestamp),responses=raw,admissions=admissions,retained=None,document=M['current_document'](fixture),serverPopulation=current,finished=[dict(serial=5,phase=p,applied=a) for p,a in [('write',False)]+([('check',False)] if uncertain else [])+[('check',True)]]))
class Oracle(unittest.TestCase):
    def test_four_exact_receipts(self):
        for f in M['FIXTURES']:M['validate_wire'](receipt(f))
    def test_minimal_form_parent_and_no_seed_are_exact(self):
        for field in [['parent_record_id','FOREIGN'],['initial_fields','[]'],['view_id','V_CUSTOM'],['cursor','invented']]:
            r=receipt();r['ledger']['requests'][5]['fields'].append(field)
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_replay_missing_and_wrong_clock(self):
        for mode in ('replay','missing','clock'):
            r=receipt()
            if mode=='replay':r['ledger']['requests'].append(copy.deepcopy(r['ledger']['requests'][5]))
            elif mode=='missing':r['ledger']['requests'].pop()
            else:r['retainedReceipt']['timestamp']='1001-0000-0123456789abcdef'
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_absent_child_grandchild_cursor_unknowns_are_preserved(self):
        for mode in ('child','grandchild','cursor','opaque'):
            r=receipt();p=r['finalPopulation']
            if mode=='child':p['records']=[x for x in p['records'] if x['id']!='R_CHILD_A']
            elif mode=='grandchild':p['records']=[x for x in p['records'] if x['id']!='R_GRANDCHILD']
            elif mode=='cursor':p['response_metadata']['next_cursor']=''
            else:p.pop('opaque_population')
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_lost_ack_cannot_claim_new_blank_identity(self):
        for key,value in [('explicitOpen',True),('positiveReplyRetained',True),('unknownIDNotGuessed',False)]:
            r=receipt('slack-list-subitem-uncertain');r[key]=value
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_obsolete_read_cannot_claim_projection_or_same_owner_epoch(self):
        for mode in ('finish','epoch','job'):
            r=receipt('slack-list-subitem-uncertain')
            if mode=='finish':r['ledger']['finished'][1]['applied']=True
            else:r['ledger']['admissions'][7][mode]=r['ledger']['admissions'][6][mode]
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_raw_parent_ack_and_lifecycle_are_not_optional(self):
        for mode in ('parent','opens','cells','title'):
            r=receipt()
            if mode=='parent':r['ledger']['responses'][5]['record']['parent_record_id']='FOREIGN'
            elif mode=='opens':r['recordOpenPrepared']=0
            elif mode=='cells':r['cellWriteRequests']=1
            else:r['after']['title']='Changed'
            with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_every_stage_requires_exact_ordered_controls_and_disabled_state(self):
        for stage in set(M['STAGES']+M['UNCERTAIN_STAGES']):
            base=dict(controls=[dict(id=i,disabled=d,hit=True) for i,d in M['expected_controls'](stage)],addDisabled=None if stage in ('initial','list-return','child') else stage not in ('parent','parent-return'))
            M['validate_controls'](stage,base)
            for mode in ('missing','extra','reordered','disabled','add'):
                g=copy.deepcopy(base)
                if mode=='missing':g['controls'].pop()
                elif mode=='extra':g['controls'].append(dict(id='unexpected',disabled=False,hit=True))
                elif mode=='reordered':g['controls'].reverse()
                elif mode=='disabled':g['controls'][-1]['disabled']=not g['controls'][-1]['disabled']
                else:g['addDisabled']=not g['addDisabled']
                with self.assertRaises(RuntimeError):M['validate_controls'](stage,g)
    def test_each_read_group_binds_all_owner_stamps_and_visit_receipt(self):
        for fixture in M['FIXTURES']:
            for field in ('job','epoch','visit','detail','selection'):
                r=receipt(fixture);r['ledger']['admissions'][-2][field]+=99
                with self.assertRaises(RuntimeError):M['validate_wire'](r)
        r=receipt('slack-list-subitem-uncertain');r['visitGenerations']=[11,21]
        with self.assertRaises(RuntimeError):M['validate_wire'](r)
    def test_parent_return_requires_full_row_and_label_inside_both_scrollports(self):
        row=dict(id='R_SERVER_SUBITEM',fullyVisible=True,labelVisible=True,hit=True,rect=[10,20,200,52],labelRect=[34,25,160,45],clipRects=[[0,0,220,60],[8,10,210,55]])
        M['validate_returned_row']('parent-return',dict(focusedRow=row))
        for mode in ('missing','identity','flag','rowclip','labelclip','nan','clipmissing'):
            bad=copy.deepcopy(row)
            if mode=='missing':bad=None
            elif mode=='identity':bad['id']='R_CHILD_A'
            elif mode=='flag':bad['fullyVisible']=False
            elif mode=='rowclip':bad['clipRects'][0][3]=30
            elif mode=='labelclip':bad['labelRect'][3]=70
            elif mode=='nan':bad['rect'][0]=float('nan')
            else:bad['clipRects'].pop()
            with self.assertRaises(RuntimeError):M['validate_returned_row']('parent-return',dict(focusedRow=bad))
    def test_readiness_and_pointer_cleanup_fail_closed(self):
        valid=M['ready_tuple'](M['FIXTURE'],'a'*32,7);M['validate_ready'](valid,valid)
        with self.assertRaises(RuntimeError):M['validate_ready']({**valid,'pid':8},valid)
        with self.assertRaises(RuntimeError):M['validate_pointer_cleanup']({'movement_observed':False,'restore_attempted':True,'restore_observed':False})
if __name__=='__main__':unittest.main()
