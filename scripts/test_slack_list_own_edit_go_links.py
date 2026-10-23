"""Focused synthetic receipt mutations; no native launch or expected-runtime borrowing."""
import copy
import hashlib
import runpy
import unittest
from pathlib import Path
import slack_list_own_edit_go_links as V
BASE=Path(__file__).with_name('test_slack_list_own_edit_plain.py')
s=BASE.read_text(encoding='utf-8').split('class PlainTests')[0]
s=s.replace('import slack_list_own_edit_plain as V','import slack_list_own_edit_go_links as V').replace('Changed plain comment',V.TEXT).replace("'own-edit-plain'",repr(V.CASE))
B={'__file__':str(BASE),'__name__':'_isolated_go_link_samples'}
exec(compile(s,str(BASE)+'[literal go-link sample]','exec'),B)
D=B['D']

def sample(narrow=False):
    fixture,r,closed=B['sample'](False,narrow);row=r['stages'][1];mount=row['proof']['opens'][0]['mount'];scope=mount['scope'];generation=mount['generation'];rect=row['geometry']['edit']['rect'];seed=V.seed_value(r['stages'][0]['model']['comments'])
    def selection():return dict(collapsed=True,contained=True,anchorOffset=1,focusOffset=1,text='')
    def ticket(text):return dict(ops=V.literal(text),mode='PlainText',revision=10,generation=generation,composing=False,pending=False,attachmentEffects='none-v1')
    def event(action,before,after):return dict(ok=True,action=action,scope=scope,generation=generation,sameNode=True,before=ticket(before),after=ticket(after),exposed=dict(rect=rect,visible=True,hit=True))
    def settled(text):
        inv=copy.deepcopy(row['editInventory']);inv['plain_delta']['ops']=V.literal(text)
        return dict(native=dict(event('state',text,text),selection=selection(),controls=dict(saveDisabled=False,cancelDisabled=False),anchors=[]),inventory=inv)
    def key(name,before,after):
        e=dict(event('key',before,after),key=name,keyCode={'Space':32,'ShiftEnter':13,'Undo':90,'Redo':90}[name],isTrusted=False,defaultPrevented=name!='Space',defaultApplied=name=='Space',shiftKey=name in ('ShiftEnter','Redo'),ctrlKey=name in ('Undo','Redo'),altKey=False,metaKey=False,selectionBefore=selection(),selectionAfter=selection())
        return dict(event=e,settled=settled(after))
    text=V.TEXT;nl=text+' \n';tail=nl+'go/team'
    proof=dict(space=key('Space',text,text+' '),newline=key('ShiftEnter',text+' ',nl),historyPauseMillis=1101,
        continuation=dict(event=dict(event('insert',nl,tail),inputCommand='insertText',text='go/team',composing=False,selectionBefore=selection(),selectionAfter=selection()),settled=settled(tail)),
        undo=key('Undo',tail,nl),redo=key('Redo',nl,tail),reset=dict(ok=True,action='native-select-all-insertText',text=text,sameNode=True,before=ticket(tail),after=ticket(text),exposed=dict(rect=rect,visible=True,hit=True)),resetSettled=settled(text))
    for stage in r['stages'][1:]:stage['proof']['goLinks']=copy.deepcopy(proof)
    r['proof']['goLinks']=copy.deepcopy(proof)
    return fixture,r,closed

class GoLinksTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,png=False)
    def mutate(self,change):
        def all_rows(r):
            for row in r['stages'][1:]:change(row['proof']['goLinks'])
            change(r['proof']['goLinks'])
        self.reject(all_rows)
    def test_complete_both_widths_and_closed_database(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_independent_authored_future_bytes(self):
        p=Path('src/desktop/slack_lists/comments/fixture/plain/go_links-future.json')
        self.assertEqual(p.read_bytes(),V.FUTURE_BYTES.encode('utf-8'))
    def test_original_policy_requires_on_not_truthy(self):
        self.reject(lambda r:r['stages'][1]['model']['fixture']['policy'][2]['response']['assignments'].update(detect_go_links={'group':True}))
    def test_plain_source_locale_and_membership_remain_required(self):
        self.reject(lambda r:r['stages'][1]['plain']['canonical'].update(current=False))
    def test_space_must_be_literal_default_not_link_conversion(self):self.mutate(lambda p:p['space']['event'].update(defaultPrevented=True,defaultApplied=False))
    def test_newline_requires_shift_enter_actual_handler(self):self.mutate(lambda p:p['newline']['event'].update(shiftKey=False))
    def test_no_manufactured_default_newline(self):self.mutate(lambda p:p['newline']['event'].update(defaultApplied=True))
    def test_no_link_attribute_in_native_delta(self):self.mutate(lambda p:p['space']['event']['after']['ops'][0].update(attributes={'link':'https://go/team'}))
    def test_actual_anchor_is_refused(self):self.mutate(lambda p:p['newline']['settled']['native'].update(anchors=[{'text':'go/team','href':'https://go/team'}]))
    def test_history_interval_and_exact_undo_required(self):
        self.mutate(lambda p:p.update(historyPauseMillis=999))
        self.mutate(lambda p:p['undo']['event']['after'].update(ops=V.literal(V.TEXT)))
    def test_redo_cannot_borrow_other_generation(self):self.mutate(lambda p:p['redo']['event'].update(generation='foreign'))
    def test_foreign_selection_refused(self):self.mutate(lambda p:p['continuation']['event']['selectionBefore'].update(contained=False))
    def test_continuation_cannot_be_semantic_delta_injection(self):self.mutate(lambda p:p['continuation']['event'].update(inputCommand='setContents'))
    def test_controls_must_be_painted_enabled(self):self.mutate(lambda p:p['resetSettled']['native']['controls'].update(saveDisabled=True))
    def test_late_queue_cannot_pass_settled(self):self.mutate(lambda p:p['newline']['settled']['native']['after'].update(pending=True))
    def test_durable_probe_ops_and_reply_are_independent(self):
        self.mutate(lambda p:p['redo']['settled']['inventory']['plain_delta'].update(ops=[]))
        self.reject(lambda r:r['stages'][1]['journal'].update(blocks=[]))
    def test_exact_one_update_and_readback(self):
        self.reject(lambda r:r['stages'][3]['model']['fixture']['updates'].clear())
        self.reject(lambda r:r['stages'][5]['ordered'].update(futureBytes='normalized'))
    def test_closed_database_cannot_drop_attempt(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_wrapper_isolated_and_baseline_unchanged(self):
        paths=[Path('scripts/slack_list_own_edit_plain.py'),Path('scripts/check-list-own-edit-plain-native.py')]
        before=[p.read_bytes() for p in paths]
        h=runpy.run_path('scripts/check-list-own-edit-go-links-native.py')['harness']()
        self.assertEqual(h['case_contract'](V.CASE)[0],V.STAGES[V.CASE]);self.assertEqual(before,[p.read_bytes() for p in paths])
    def test_outer_and_inner_dispatch_with_original_assignment_source(self):
        root=Path('src/desktop/slack_lists')
        self.assertIn('"own-edit-plain-go-links"',(root/'check/compose_comments.rs').read_text())
        self.assertIn('return go_links::run(state, d).await',(root/'check/compose_comments/own_edit/plain.rs').read_text())
        self.assertIn('go_links::apply_assignments',(root/'comments/fixture/own_edit/policy.rs').read_text())
        self.assertIn('go_links::apply_assignments',(root/'comments/fixture/completion/files.rs').read_text())

if __name__=='__main__':unittest.main()
