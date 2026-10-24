"""Independent original-history and controller chronology checks; no native launch."""
import base64
import copy
import html
import json
from pathlib import Path
import subprocess
import unittest

import canvas_sequence_input as I
import canvas_sequence_oracle as N
import canvas_sequence_wire as W
import test_canvas_sequence_fixtures as F

ROOT = Path(__file__).resolve().parent.parent
REF = ROOT / 'tmp/canvas-sequence-revisit-20261023/reference'


def controller_plans():
    source = "import {actionPlan} from './scripts/canvas_sequence_input.mjs';console.log(JSON.stringify(['ascii','unicode'].flatMap(corpus=>['correction','erase','revert'].flatMap(kind=>[3,8].map(count=>({corpus,cadence:'revisit-'+kind,count,actions:actionPlan(corpus,count,'revisit-'+kind)}))))));"
    return json.loads(subprocess.run(['node', '--input-type=module', '-e', source], cwd=ROOT,
        text=True, encoding='utf-8', capture_output=True, check=True, timeout=30).stdout)


def witness(plan):
    """Derive input events from actual controller actions, not oracle expectations."""
    seed = 'Keep this' if plan['corpus'] == 'ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600'
    parts, events, active, selection = [seed], [], 0, (W.T.utf16(seed),)*2
    for action in plan['actions']:
        if action['type'] == 'click':
            active = action['activeIndex']
            continue
        if action['type'] == 'select':
            selection = (action['start'], action['end'])
            continue
        before = parts[active]
        start, end = selection
        if action['type'] == 'enter':
            assert active == len(parts)-1
            kind, data, after = 'insertParagraph', None, ''
            parts.append(''); active += 1; caret = 0
        elif action['type'] == 'insert':
            kind, data, after = 'insertText', action['text'], action['after']
            caret = start+W.T.utf16(data); parts[active] = after
        else:
            kind, data, after = 'deleteContentBackward', None, action['after']
            if start == end: start -= 1
            caret = start; parts[active] = after
        sections = [dict(text=p, html=html.escape(p, quote=False) if p else '<br>',
            placeholder=None if p else dict(html='<br>', spanDepth=0)) for p in parts]
        events.append(dict(kind='enter' if action['type']=='enter' else 'edit', activeIndex=active,
            inputType=kind, data=data, selectionStart=selection[0], selectionEnd=selection[1], start=start, end=end,
            beforeText=before, afterText=after, beforeRevision=len(events), revision=len(events)+1, caret=caret,
            beforeInputTrusted=True, inputTrusted=True, after=F.blocks(parts), rawAfter=F.blocks(parts), sections=sections,
            html='<div class="rich-editor-block">'+''.join('<div class="rich-editor-section">'+s['html']+'</div>' for s in sections)+'</div>'))
        selection = (caret, caret)
    return dict(kind='sequence', generation='1', visit='0', before=F.blocks([seed]), offset=W.T.utf16(seed),
        after=events[0]['after'], rawAfter=events[0]['rawAfter'], revision=1, beforeInputTrusted=True,
        inputTrusted=True, inputType='insertParagraph', sequence=dict(revision=len(events), events=events,
        **{k:events[-1][k] for k in ('after','rawAfter','html')}))


class RevisitTests(unittest.TestCase):
    def test_original_per_report_patches_and_full_first_member_history(self):
        vectors = json.loads((REF/'cadence-bindings.json').read_text(encoding='utf-8'))['vectors']
        self.assertEqual(len(vectors),12)
        for v in vectors:
            with self.subTest(vector=v['vector']):
                e = W.config(3,v['mode'],v['layout'],'revisit-'+v['revisit'].replace('exact-',''),v['corpus'])
                member = v['members'][0]
                self.assertEqual(list(e.reports[0]),member['orderedChangedReports'])
                self.assertEqual(W.patch_groups(e,0),[
                    [p for group in r['patches'] for p in group] for r in member['reportPatches'] if not r['unchanged']])
                row = W.S.raw(W.S.messages(W.decoded(v['wireBase64']),1)[1])
                source = None
                if e.reused:
                    prior = next(s for s in v['sourceEntities'] if s['id']=='following')
                    session = F.F.replace(F.F.replace(base64.b64decode(prior['session']),1,e.following),4,8_000_000_000_000_000)
                    source = W.wire(F.F.f(1,e.following)+F.F.f(38,session))
                    current = F.F.replace(W.field(W.wire(row),38,2),1,e.following)
                    row = F.F.replace(F.F.replace(row,1,e.following),38,current)
                W.member_history(W.wire(row),source,e,0,'0123456789ab')
                # Exact revert retains its historical mutation steps.
                if v['revisit']=='exact-revert':
                    self.assertEqual(e.reports[0][0],e.reports[0][-1]); self.assertEqual(len(W.patch_groups(e,0)),3)

    def test_controller_chronology_and_all_non_target_snapshot_guards(self):
        for plan in controller_plans():
            with self.subTest(corpus=plan['corpus'],cadence=plan['cadence'],count=plan['count']):
                e = W.config(plan['count'],cadence=plan['cadence'],corpus=plan['corpus'])
                value = witness(plan); N.witness(value,e)
                events = value['sequence']['events']
                self.assertEqual([x['activeIndex'] for x in events[:5]],[1,1,2,2,1])
                for key,bad_value in [('activeIndex',2),('revision',99),('beforeInputTrusted',False),('start',0)]:
                    if key=='start' and plan['cadence']=='revisit-erase':continue
                    bad=copy.deepcopy(value); bad['sequence']['events'][4][key]=bad_value
                    with self.assertRaises(RuntimeError):N.witness(bad,e)
                # A later non-target row may not silently acquire even plain span wrapping.
                bad=copy.deepcopy(value); event=bad['sequence']['events'][4]
                event['sections'][2]['html']='<span>'+event['sections'][2]['html']+'</span>'
                event['html']='<div class="rich-editor-block">'+''.join('<div class="rich-editor-section">'+s['html']+'</div>' for s in event['sections'])+'</div>'
                with self.assertRaisesRegex(RuntimeError,'non-target'):N.witness(bad,e)
                # Reordering by member would erase real chronology and must fail.
                bad=copy.deepcopy(value); bad['sequence']['events'].sort(key=lambda x:x['activeIndex'])
                with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_python_admission_and_count_guards(self):
        for cadence in ('revisit-correction','revisit-erase','revisit-revert'):
            for count in range(2,17):
                value=dict(ok=True,fixture='canvas-structure',nonce='a'*32,ordinal=0,corpus='ascii',mode='sequence',typing='none',
                    sequenceHistory='bootstrap',sequenceLayout='no-following',sequenceCount=count,sequenceCadence=cadence,
                    fieldId='canvas-text-draft',seed='Keep this',offset=9,revision=0,generation='1',visit='0')
                args=('canvas-structure','a'*32,0,'bootstrap','no-following','ascii',count,cadence)
                if count==2:
                    with self.assertRaises(RuntimeError):I.validate_request(value,*args)
                    with self.assertRaises(RuntimeError):W.config(count,cadence=cadence)
                else:
                    I.validate_request(value,*args);W.config(count,cadence=cadence)
                    for key,new in [('sequenceCount',True),('sequenceCadence','straight'),('ordinal',1),('generation',1),('visit','bad'),('typing','correction')]:
                        with self.assertRaises(RuntimeError):I.validate_request({**value,key:new},*args)

    def test_cli_refuses_before_launch(self):
        for script in ('check-canvas-split-native.py','check-canvas-split-recovery-native.py'):
            common=['--fixture','canvas-structure' if script=='check-canvas-split-native.py' else 'canvas-local-review','--captures',str(ROOT/'tmp/must-not-create-revisit-cli')]
            if script=='check-canvas-split-native.py':common+=['--scenario','success']
            cases=[]
            for cadence in ('revisit-correction','revisit-erase','revisit-revert'):
                cases.extend([
                    (['--mode','sequence','--sequence-cadence',cadence],'at least 3 appends'),
                    (['--mode','middle','--sequence-count','3','--sequence-cadence',cadence],'require sequence mode'),
                    (['--mode','sequence','--sequence-count','3','--typing','correction','--sequence-cadence',cadence],'legacy typing selectors')])
            cases.append((['--mode','sequence','--sequence-count','4','--sequence-cadence','correction-before-enter'],'pinned 2, 3, 8 or 16'))
            for args,message in cases:
                run=subprocess.run(['python',str(ROOT/'scripts'/script),*common,*args],cwd=ROOT,capture_output=True,text=True,encoding='utf-8',timeout=20)
                self.assertEqual(run.returncode,2);self.assertIn(message,run.stderr)
            self.assertFalse((ROOT/'tmp/must-not-create-revisit-cli').exists())


if __name__=='__main__':unittest.main()
