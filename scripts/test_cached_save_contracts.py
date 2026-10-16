"""Pure schema/path/helper regressions; no native or network."""
import copy,json,tempfile,unittest
from pathlib import Path
from cached_save_contracts import *
from test_toast_geometry import sample_toast
from test_save_workflow import observed as prior_observed
EXPECTED=owner('a'*32,123,456)
def stage(i):
    name=STAGES[i];issued,returns,outcomes,held,released,reads=((0,0,0,False,False,0),(1,1,0,False,False,0),(2,2,1,False,False,0),(3,3,1,True,False,0),(3,3,1,True,False,1),(3,3,2,True,True,1))[i]
    fresh=i>=4;text=NEW if fresh else OLD;toast='Saved release-notes.txt' if i==2 else None
    return dict(ok=True,identity=identity(EXPECTED),stage=name,
      dom=dict(ok=True,stage=name,viewport=[1280,780],text=text,replaced=fresh,toast=sample_toast(toast) if toast else None,
        button=dict(label='Cancel' if i==3 else 'Download',disabled=False,hit=True,rect=dict(left=500,top=500,right=600,bottom=530,width=100,height=30))),
      ledger=dict(issued=issued,returns=[[1,'none'],[2,'existing'],[3,'existing']][:returns],outcomes=[[2,'saved'],[3,'retired-before-transfer']][:outcomes],transfers=[2] if issued>=2 else [],held=held,released=released,metadata=reads,bodies=reads,active=3 if i in (3,4) else None,failed=False),
      observed=dict(old='super-platinum-media://attachment/'+'a'*32,current='super-platinum-media://attachment/'+('b' if fresh else 'a')*32,fresh=fresh,oldCanceled=fresh,bytes=text,syntheticSession=True,structurallyDisconnected=True,toast=toast))
def terminal(stages):return dict(ok=True,identity=identity(EXPECTED),stages=stages,ledger=stages[-1]['ledger'],actualRfd=True,programmaticActivation=True,syntheticSession=True,structurallyDisconnected=True,lateCommitExercised=False)
class Contracts(unittest.TestCase):
    def test_closed_actual_paths_and_one_transfer(self):
        stages=[stage(i) for i in range(6)];ident=identity(EXPECTED);root=Path('owned')
        paths=[None,str(root/'downloads'/('overwrite-'+'a'*32+'.txt')),str(root/'downloads'/('retire-'+'a'*32+'.txt'))]
        data=dict(waiting=[ident],stage=stages,check=[terminal(stages)],requested=[dict(identity=ident,attempt=i,actualRfd=True,cached=True,mode='ConfirmedCachedOverwrite') for i in (1,2,3)],
            returned=[dict(identity=ident,attempt=i,kind='none' if i==1 else 'existing',actualRfd=True,path=paths[i-1],accepted=True) for i in (1,2,3)],
            transfer=[dict(identity=ident,attempt=2,cached=True)],held=[dict(identity=ident,attempt=3,actualReturnedPath=True,beforeCurrentCheck=True)],
            outcome=[dict(identity=ident,attempt=2,kind='saved',ok=True),dict(identity=ident,attempt=3,kind='retired-before-transfer',ok=True,canceled=True)])
        validate_logs(data,EXPECTED,stages,root)
        for mutate in [lambda d:d['returned'][2].update(path=paths[1]),lambda d:d['transfer'].append(dict(identity=ident,attempt=3,cached=True)),lambda d:d['outcome'][1].update(canceled=1),lambda d:d['held'].append(d['held'][0])]:
            bad=copy.deepcopy(data);mutate(bad)
            with self.assertRaises(RuntimeError):validate_logs(bad,EXPECTED,stages,root)
    def test_stage_terminal_and_strict_nested_echo(self):
        stages=[stage(i) for i in range(6)];t=terminal(stages);validate_terminal(t,stages,EXPECTED)
        for mutate in [lambda v:v['stages'][0].update(ok=1),lambda v:v['ledger'].update(released=1),lambda v:v.update(lateCommitExercised=True)]:
            bad=copy.deepcopy(t);mutate(bad)
            with self.assertRaises(RuntimeError):validate_terminal(bad,stages,EXPECTED)
    def test_cancellation_needs_retired_outcome_no_extra_transfer(self):
        for mutate in [lambda v:v['ledger'].update(transfers=[2,3]),lambda v:v['ledger'].update(outcomes=[[2,'saved']]),lambda v:v['observed'].update(oldCanceled=False),lambda v:v['ledger'].update(metadata=True),lambda v:v['observed'].update(bytes=OLD)]:
            s=stage(5);mutate(s)
            with self.assertRaises(RuntimeError):validate_stage(s,5,EXPECTED)
    def test_saved_notice_requires_actual_visible_proof(self):
        s=stage(2);s['dom']['toast']['visible']=False
        with self.assertRaises(RuntimeError):validate_stage(s,2,EXPECTED)
    def test_same_source_identity_across_terminal(self):
        stages=[stage(i) for i in range(6)];stages[5]['observed']['old']='super-platinum-media://attachment/'+'c'*32
        with self.assertRaises(RuntimeError):validate_terminal(terminal(stages),stages,EXPECTED)
    def test_cached_title_and_review_retire_path_are_exact(self):
        observed,review=prior_observed('save-new');observed['stage']=review['stage']='save-retire';observed['probe']['window']['title']='Save file'
        review['request']['allowedPath']=observed['allowedPaths'][1]
        validate_probe(observed['probe'],EXPECTED,'save',77);validate_review(review,observed)
        review['request']['allowedPath']=observed['allowedPaths'][0]
        with self.assertRaises(RuntimeError):validate_review(review,observed)
        observed['probe']['window']['title']='Save original file - choose a new filename'
        with self.assertRaises(RuntimeError):validate_probe(observed['probe'],EXPECTED,'save',77)
    def test_accept_retire_preserves_observed_role(self):
        observed,review=prior_observed('accept','overwrite');observed['stage']=review['stage']='accept-retire'
        validate_review(review,observed);review['request']['role']='save'
        with self.assertRaises(RuntimeError):validate_review(review,observed)
    def test_private_sentinels_exact_cached_write_and_retirement(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/('cached-viewer-save-'+'a'*32);root.mkdir();(root/'downloads').mkdir()
            audit=DestinationAudit(root,'a'*32);audit.create_sentinel();audit.snapshot('initial')
            audit.paths[0].write_bytes(OLD.encode());audit.snapshot('saved',saved=True)
            audit.paths[1].write_bytes(b'changed')
            with self.assertRaises(RuntimeError):audit.snapshot('retired',saved=True)
    def test_old_original_helper_bytes_and_mode_are_distinct(self):
        p=Path(__file__).with_name('windows_cached_save.ps1');s=p.read_text()
        self.assertNotIn('Save original file - choose a new filename',s)
        self.assertIn('cached-viewer-save-',s);self.assertIn('Cached destination sentinel must already exist',s)
if __name__=='__main__':unittest.main()
