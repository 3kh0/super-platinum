"""Synthetic receipt mutations; no native app or fabricated product conversion result."""
import copy
import runpy
import unittest
from pathlib import Path
import slack_list_own_edit_emoticon as V
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_plain.py')))
D=B['D']

def sample(off=False,narrow=False):
    fixture,base,closed=B['sample'](False,narrow)
    case=V.CASES[int(off)];rows=[];first=base['stages'][0];initial=first['model']['comments'];scope=first['journal']['scope'];seed=V.seed_value(initial)
    rect=copy.deepcopy(base['stages'][1]['geometry']['edit']['rect']);viewport=first['geometry']['viewport']
    def ticket(ops,gen=78,composing=False):return dict(ops=copy.deepcopy(ops),mode='PlainText',revision=1,generation=str(gen),composing=composing,pending=False,attachmentEffects='none-v1')
    def inv(ops=None):
        v=copy.deepcopy(first['editInventory'])
        if ops is not None:v.update(seed=copy.deepcopy(seed),revision=1,capture='00000000-0000-4000-8000-000000000003',plain_delta=dict(version=1,ops=copy.deepcopy(ops)))
        return v
    def button(id):return dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=rect))
    def selection(collapsed=True,text=''):return dict(contained=True,collapsed=collapsed,anchorOffset=8,focusOffset=8 if collapsed else 6,text=text)
    exposed=dict(visible=True,hit=True,rect=rect)
    def event(action,before,after,gen=78,composing=False,after_composing=False):return dict(ok=True,action=action,scope=f'list-edit:100:{gen}',generation=str(gen),sameNode=True,before=ticket(before,gen,composing),after=ticket(after,gen,after_composing),exposed=exposed)
    def key(name,before,after,consumed=True,default=False,gen=78,composing=False,collapsed=True):
        return dict(event('key',before,after,gen,composing,composing),key=name,keyCode={'Space':32,'Enter':13,'Undo':90,'Redo':90}[name],isTrusted=False,defaultPrevented=consumed,defaultApplied=default,ctrlKey=name in ('Undo','Redo'),shiftKey=name=='Redo',selectionBefore=selection(collapsed),selectionAfter=selection())
    def input(text,before,after,gen=78):return dict(native=dict(ok=True,action='native-select-all-insertText',text=text,sameNode=True,before=ticket(before,gen),after=ticket(after,gen),exposed=exposed),inventory=inv(after))
    p=copy.deepcopy(first['proof']);p.update(emoticonsEnabled=not off,opens=[],inputs=[],actions=[])
    requests=[];pages=[]
    for i,name in enumerate(V.STAGES[case]):
        ops=V.ops_for(name);editing=ops is not None;saving=name=='saving';accepted=name in ('saved','readback');sent=saving or accepted
        index=0 if name=='draft' else 3 if saving else 4 if name=='saved' else 5 if name=='readback' else 2 if name=='cancelled' else 1
        row=copy.deepcopy(base['stages'][index]);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'));m=row['model'];c=m['comments'];n=V.READS[case][i]
        c['comments'].update(loading=saving,page=copy.deepcopy(V.FUTURE if name=='readback' else V.INITIAL),pageMessages=copy.deepcopy((V.FUTURE if accepted else V.INITIAL)['messages']))
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(initial['ledger']['requests']*(n//4))],completed=n,cancelled=0,held=False,replies=n//4)
        f=m['fixture'];f.update(policy=[dict(request=q,response=r) for _ in range(V.POLICIES[case][i]) for q,r in V.policy_cycle(not off)],prepared=[V.O['wire'](V.BODY)] if sent else [],updates=[V.O['wire'](V.BODY)] if sent else [],responses=[dict(disposition='accepted',raw=V.ACK)] if sent else [],futureRows=[V.FUTURE['messages'][1]] if sent else [])
        if sent:
            a=m['edit']['attempts'][0];a.update(blocks=V.FUTURE['messages'][1]['blocks'],receipt=None if saving else V.ACK)
        gen=79 if sent else 78
        if editing:m['edit']['active']['plainDelta']=dict(version=1,ops=copy.deepcopy(ops))
        while len(p['opens'])<(0 if i==0 else 2 if sent else 1):
            op=copy.deepcopy(base['stages'][1 if len(p['opens'])==0 else 3]['proof']['opens'][-1]);p['opens'].append(op)
        if name=='editing':p['inputs'].append(input('Happy :)',V.INITIAL_OPS,V.HAPPY))
        if name in ('space','literal'):
            p['historyWaitMillis']=1101;p['space']=key('Space',V.HAPPY,V.SPACE if not off else V.literal('Happy :) '),not off,off)
        if name=='undo':p['undo']=key('Undo',V.SPACE,V.HAPPY)
        if name=='redo':p['redo']=key('Redo',V.HAPPY,V.SPACE)
        if name=='guards':
            guards=[]
            before=V.SPACE
            for kind,text in [('code','`Happy :)'),('url','https://example.test/:)')]:
                p['inputs'].append(input(text,before,V.literal(text)));guards.append(dict(kind=kind,key=key('Space',V.literal(text),V.literal(text+' '),False,True)));before=V.literal(text+' ')
            p['inputs'].append(input('Happy :)',before,V.HAPPY));select=dict(event('select',V.HAPPY,V.HAPPY),start=6,end=8,selection=selection(False,':)'))
            guards.append(dict(kind='selection',selection=select,key=key('Space',V.HAPPY,V.literal('Happy  '),False,True,collapsed=False)))
            p['inputs'].append(input('Happy :)',V.literal('Happy  '),V.HAPPY))
            start=dict(event('composition',V.HAPPY,V.HAPPY,after_composing=True),active=True,isTrusted=False,selection=selection())
            end=dict(event('composition',V.HAPPY,V.HAPPY,composing=True),active=False,isTrusted=False,selection=selection())
            guards.append(dict(kind='composition',start=start,key=key('Space',V.HAPPY,V.HAPPY,False,composing=True),end=end));p['guards']=guards
        if name=='cancelled':p['cancel']=button('slack-list-comment-edit-cancel')
        if saving:
            p['inputs'].append(input('Enter :)',V.INITIAL_OPS,V.literal('Enter :)'),79));p['enter']=key('Enter',V.literal('Enter :)'),V.ENTER,gen=79);p['doubleSave']=dict(ok=True,disabled=True)
        if name=='saved':p['release']=V.O['RELEASE']
        if name=='readback':p['actions'].append(button('slack-list-comments-refresh'))
        row['editInventory']=inv(ops)
        if saving:
            a=copy.deepcopy(base['stages'][3]['editInventory']['attempt']);a['record'].update(blocks=V.FUTURE['messages'][1]['blocks'],text=V.TEXT,plain_delta=dict(version=1,ops=V.ENTER));row['editInventory']['attempt']=a
        while len(requests)<n//4:
            j=len(requests);future=name=='readback';text=V.FUTURE_BYTES if future else V.INITIAL_BYTES;raw=V.W['decoded'](text)
            requests.append(dict(serial=4*(j+1),readSerial=j+1,method='conversations.replies',args=initial['ledger']['requests'][3]['args'],variant='future' if future else 'initial',bytes=text,raw=raw,owner=dict(serial=initial['comments']['generation'],account=initial['account'],epoch=initial['epoch'])))
            pages.append(dict(raw=raw,messages=raw['messages'],witness=dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None if future else ['italic','bold']),current=True,requestSerial=4*(j+1),readSerial=j+1))
        entry=dict(raw=c['comments']['page'],messages=c['comments']['pageMessages'],witness=dict(messageIndex=1,raw=c['comments']['page']['messages'][1],styleKeys=None if name=='readback' else ['italic','bold']))
        row['ordered'].update(futureBytes=V.FUTURE_BYTES,requests=copy.deepcopy(requests),pages=copy.deepcopy(pages),entryPage=entry,completed=n//4)
        row['plain']['preferences']=V.prefs(not off);row['plain']['completion']['seed']['boot']['prefs']=V.prefs(not off)['prefs']
        g=row['geometry'];g['row']['text']=(V.TEXT if accepted else V.INITIAL['messages'][1]['text'])+' Preserved original context'
        if editing:g['edit'].update(delta=ticket(ops,gen),text=''.join(o['insert'] if isinstance(o['insert'],str) else '🙂' for o in ops).rstrip('\n'))
        row['proof']=copy.deepcopy(p);rows.append(row)
    p['closedDisk']=dict(copy.deepcopy(p['originalDisk']),edit=copy.deepcopy(rows[-1]['editInventory']))
    receipt=dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,proof=p,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    history=[]
    if not off:history=[dict(copy.deepcopy(rows[7]['editInventory']['attempt']),phase='Confirmed',raw=V.ACK)]
    closed.update(editInventories=[rows[-1]['editInventory']],editHistory=history)
    return copy.deepcopy((fixture,receipt,closed))

class EmoticonTests(unittest.TestCase):
    def reject(self,change,off=False):
        f,r,_=sample(off);change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,expected_case=r['case'],png=False)
    def test_complete_cases_widths_and_closed_database(self):
        for off in (False,True):
            for narrow in (False,True):
                f,r,c=sample(off,narrow);V.validate(f,r,D,expected_case=r['case'],png=False);V.validate_closed(c,r)
    def test_authored_bytes_including_newline(self):
        self.assertEqual(Path('src/desktop/slack_lists/comments/fixture/plain/emoticon-future.json').read_bytes(),V.FUTURE_BYTES.encode())
    def test_foreign_capture_owner(self):self.reject(lambda r:r['stages'][2]['proof']['space'].update(scope='other'))
    def test_wrong_native_marker(self):self.reject(lambda r:r['stages'][2]['geometry']['edit']['delta'].update(attachmentEffects='unknown'))
    def test_painted_delta_cannot_borrow_other_generation(self):self.reject(lambda r:r['stages'][2]['geometry']['edit']['delta'].update(generation='999'))
    def test_literal_shortcode_not_semantic_conversion(self):self.reject(lambda r:r['stages'][2]['geometry']['edit']['delta'].update(ops=V.literal('Happy :slightly_smiling_face: ')))
    def test_double_default_insertion(self):self.reject(lambda r:r['stages'][2]['proof']['space'].update(defaultApplied=True))
    def test_missing_default_on_off(self):self.reject(lambda r:r['stages'][2]['proof']['space'].update(defaultApplied=False),True)
    def test_preference_off_not_inferred_from_empty_output(self):self.reject(lambda r:r['stages'][1]['plain']['preferences']['prefs'].update(convert_emoticons=True),True)
    def test_history_boundary_measured(self):self.reject(lambda r:r['stages'][2]['proof'].update(historyWaitMillis=999))
    def test_undo_cannot_use_redo_modifier(self):self.reject(lambda r:r['stages'][3]['proof']['undo'].update(shiftKey=True))
    def test_guard_selection_is_noncollapsed(self):self.reject(lambda r:r['stages'][5]['proof']['guards'][2]['selection']['selection'].update(collapsed=True))
    def test_composition_cannot_fabricate_default_input(self):self.reject(lambda r:r['stages'][5]['proof']['guards'][3]['key'].update(defaultApplied=True))
    def test_composition_phase_required(self):self.reject(lambda r:r['stages'][5]['proof']['guards'][3]['start']['after'].update(composing=False))
    def test_guard_actual_before_delta(self):self.reject(lambda r:r['stages'][5]['proof']['guards'][0]['key']['before'].update(ops=V.HAPPY))
    def test_returned_reply_unchanged(self):self.reject(lambda r:r['stages'][6]['geometry']['reply'].update(sameNode=False))
    def test_cancel_has_no_update(self):self.reject(lambda r:r['stages'][6]['model']['fixture']['updates'].append({}))
    def test_enter_semantic_wire_exact(self):self.reject(lambda r:r['stages'][7]['model']['fixture']['prepared'][0]['args'].update(blocks='[]'))
    def test_private_preflight_required(self):self.reject(lambda r:r['stages'][7]['ordered']['pages'].pop())
    def test_readback_original_byte_witness(self):self.reject(lambda r:r['stages'][9]['ordered'].update(futureBytes=V.FUTURE_BYTES.rstrip()))
    def test_no_extra_full_catalog(self):self.reject(lambda r:r['stages'][7]['plain'].update(customCatalog={}))
    def test_closed_history_preserves_semantic_delta(self):
        f,r,c=sample();c['editHistory'][0]['record']['plain_delta']['ops']=V.literal(V.TEXT)
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_isolated_wrapper_and_unchanged_old_cases(self):
        import sys
        sys.path.insert(0,str(Path(__file__).parent))
        m=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-emoticon-native.py')));h=m['harness']()
        for case in V.CASES:self.assertEqual(h['case_contract'](case)[0],V.STAGES[case])
        old=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
        self.assertEqual(old['CASES'],('own-edit-plain','own-edit-plain-storage'))

if __name__=='__main__':unittest.main()
