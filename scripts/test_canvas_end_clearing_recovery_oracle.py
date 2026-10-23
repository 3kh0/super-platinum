"""Portable v7 recovery custody, source/current distinction and native controls."""
import base64
import copy
import unittest

import canvas_end_clearing_recovery_oracle as O
import canvas_split_recovery_oracle as review
import test_canvas_end_clearing_oracle as F
import test_canvas_end_reuse_recovery_oracle as R


def packets(phase,corpus='ascii',typing='none',reuse_base='positive',reuse_content='spaces'):
    stages,terminal,_=R.packets(phase,corpus)
    source,write,_=F.source_and_request(corpus,typing,reuse_base,reuse_content)
    checkpoint=F.checkpoint(source,write,corpus,typing)
    original=F.source_literal(reuse_content);plain=original.replace('<br>','\n')
    kind=reuse_content if reuse_content in ('spaces','nbsp','br') else 'whitespace-br' if '<br>' in original else 'whitespace'
    label={'spaces':'3 spaces','nbsp':'Non-breaking space','br':'Line break','whitespace':'Whitespace','whitespace-br':'Whitespace and line break'}[kind]
    for stage in stages:
        stage.update(typing=typing,reuseBase=reuse_base,reuseChild=write['child'],reuseContent=reuse_content,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']=='neighbor')
            row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(checkpoint)
        preview=stage['preview'];preview['originalChild']=None
        if preview['panel']:
            final=F.literals(corpus,typing)[-1]
            preview['split'][2]['text']=final
            preview['originalChild']=dict(text=plain,kind=kind,label=label)
            preview['current'][-1]=dict(id=write['child'],deleted=False,text=plain,contentKind=kind,contentLabel=label)
            action='Clear and edit the following paragraph' if final else 'Clear and reuse the following paragraph'
            preview['warning']=' '.join(review.WARNINGS)+' '+action+' '+label+' '+label+(' Empty paragraph' if not final else '')
    terminal.update(typing=typing,reuseBase=reuse_base,reuseChild=write['child'],reuseContent=reuse_content,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,checkpoint


class ClearingRecovery(unittest.TestCase):
    def test_all_shapes_bases_and_inert_phases(self):
        for base in ('positive','zero'):
            for shape in ('spaces','nbsp','br'):
                for typing in ('none','correction','erase'):
                    for phase in O.STAGES:
                        with self.subTest(base=base,shape=shape,typing=typing,phase=phase):
                            O.validate(phase,*packets(phase,'unicode',typing,base,shape),'unicode',typing,base,shape)

    def test_original_current_and_intended_cannot_be_substituted(self):
        for target in ('originalChild','current','intended','label'):
            stages,terminal,checkpoint=packets('review',typing='correction',reuse_content='br')
            preview=stages[0]['preview']
            if target=='originalChild':preview['originalChild']['text']=''
            elif target=='current':preview['current'][-1]['text']='New paragraph'
            elif target=='intended':preview['split'][2]['text']='\n'
            else:preview['warning']=preview['warning'].replace('Line break','Empty paragraph')
            with self.subTest(target=target),self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint,typing='correction',reuse_content='br')

    def test_retained_whole_v7_pair_is_required_at_every_failure(self):
        for phase,name in [('review','cancelled'),('review','stale'),('review','account-changed'),('storage-failure','storage-failed')]:
            stages,terminal,checkpoint=packets(phase)
            next(s for s in stages if s['stage']==name)['journal']['active']=None
            with self.subTest(name=name),self.assertRaises(RuntimeError):O.validate(phase,stages,terminal,checkpoint)

    def test_no_replay_archive_account_and_version_boundaries(self):
        for target in ('replay','archive','account','version','selector'):
            stages,terminal,checkpoint=packets('review')
            if target=='replay':terminal['dispatches']=1
            elif target=='archive':stages[-2]['journal']['archivedExact']=False
            elif target=='account':next(s for s in stages if s['stage']=='account-changed')['preview']['originalChild']={'text':'old'}
            elif target=='version':checkpoint['version']=6
            else:terminal['reuseContent']='empty'
            with self.subTest(target=target),self.assertRaises(RuntimeError):O.validate('review',stages,terminal,checkpoint)


if __name__=='__main__':unittest.main()
