"""Portable inert v6 review; actual history decoded, native UI remains unrun."""
import base64
import copy
import unittest

import canvas_end_typing_recovery_oracle as O
import canvas_split_recovery_oracle as review
import test_canvas_end_typing_oracle as F
import test_canvas_end_recovery_oracle as E
import test_canvas_end_reuse_recovery_oracle as R


def packets(phase, corpus="ascii", mode="end", typing="correction"):
    stages, terminal, _ = R.packets(phase, corpus) if mode == "reuse" else E.packets(phase, corpus)
    source, write, _ = F.source_and_request(mode, corpus, typing)
    checkpoint = F.checkpoint(source, write, mode, corpus, typing)
    for stage in stages:
        stage.update(typing=typing, canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']=='neighbor')
            row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(checkpoint)
        preview=stage['preview']
        if preview['panel']:
            preview['split'][2]['text']=F.literals(corpus,typing)[-1]
            nonempty=typing=='correction';reused=mode=='reuse'
            action=('Reuse and edit the existing empty paragraph after this paragraph' if nonempty else 'Reuse the existing empty paragraph after this paragraph') if reused else (
                    'Add a paragraph after this paragraph' if nonempty else 'Add an empty paragraph after this paragraph')
            preview['warning']=' '.join(review.WARNINGS)+' '+action+' Empty paragraph'*(int(reused)+int(not nonempty))
    terminal.update(typing=typing,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,checkpoint


class TypedRecoveryOracle(unittest.TestCase):
    def test_all_inert_workflows_modes_corpora_and_net_empty(self):
        for mode in ('end','reuse'):
            for corpus in ('ascii','unicode'):
                for typing in ('correction','erase'):
                    for phase in O.STAGES:
                        with self.subTest(mode=mode,corpus=corpus,typing=typing,phase=phase):O.validate(phase,*packets(phase,corpus,mode,typing),corpus,mode,typing)

    def test_review_keeps_original_current_and_intended_child_distinct(self):
        for mode in ('end','reuse'):
            stages,terminal,checkpoint=packets('review',mode=mode)
            stages[0]['preview']['split'][2]['text']=''
            with self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint,mode=mode)
        stages,terminal,checkpoint=packets('review',mode='reuse')
        stages[0]['preview']['current'][-1]['text']='New paragraph'
        with self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint,mode='reuse')

    def test_unknown_whole_history_stays_locked_on_each_refusal(self):
        for phase,name in [('review','cancelled'),('review','stale'),('review','account-changed'),('storage-failure','storage-failed')]:
            stages,terminal,checkpoint=packets(phase)
            next(s for s in stages if s['stage']==name)['journal']['active']=None
            with self.subTest(name=name),self.assertRaises(RuntimeError):O.validate(phase,stages,terminal,checkpoint)

    def test_net_empty_cannot_reinterpret_v6_as_untyped_empty(self):
        stages,terminal,checkpoint=packets('restart',typing='erase')
        checkpoint['version']=4
        with self.assertRaises(RuntimeError):O.validate('restart',stages,terminal,checkpoint,typing='erase')
        stages,terminal,checkpoint=packets('restart',typing='erase')
        checkpoint['action']['EndTyping']['child_updates']=['']
        with self.assertRaises(RuntimeError):O.validate('restart',stages,terminal,checkpoint,typing='erase')

    def test_native_empty_label_count_and_no_replay_are_required(self):
        stages,terminal,checkpoint=packets('review',mode='reuse',typing='erase')
        stages[0]['preview']['warning']=stages[0]['preview']['warning'].replace(' Empty paragraph','',1)
        with self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint,mode='reuse',typing='erase')
        stages,terminal,checkpoint=packets('restart');terminal['dispatches']=1
        with self.assertRaises(RuntimeError):O.validate('restart',stages,terminal,checkpoint)

    def test_source_and_archive_custody_and_caller_labels_are_pinned(self):
        for target in ('source','archive','selector'):
            stages,terminal,checkpoint=packets('review')
            if target=='source':stages[2]['canonical']['sections'].pop()
            elif target=='archive':stages[-2]['journal']['archivedExact']=False
            else:terminal['typing']='erase'
            with self.subTest(target=target),self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint)


if __name__=='__main__':unittest.main()
