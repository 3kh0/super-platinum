#!/usr/bin/env python3
"""Adjacent ordinary rich-run equivalence through the original owned harness."""
import argparse
import inspect
from pathlib import Path
import runpy

CASE='own-edit-runs'
FLAG='SUPER_PLATINUM_LIST_OWN_EDIT_RUNS_CHECK'


def storage_controller(case):
    if case!=CASE:raise RuntimeError('Unexpected rich-runs storage case')
    module=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))
    parent=module['StorageControl']
    class RunsStorage(parent):
        def __init__(self,directory,expected,*,case):
            if case!=CASE:raise RuntimeError('Unexpected rich-runs storage case')
            super().__init__(directory,expected,case='own-edit-storage')
            self.command_count=0
            self.evidence.update(configuredCase=case,commandCount=0)
    module['StorageControl']=RunsStorage
    return module


def closed(snapshot,receipt):
    import slack_list_own_edit_runs as oracle
    oracle.validate_closed(snapshot,receipt)


def harness():
    loaded=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    base=loaded['run_fixture'].__globals__;parent=base['NativeFixture'];original_contract=base['case_contract']
    class RunsFixture(parent):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE')!=CASE or FLAG in checks:
                raise RuntimeError('Unexpected List rich-runs mode')
            checks[FLAG]='1';super().__init__(fixture,log,binary,checks,**kwargs)
    def contract(case):
        if case!=CASE:return original_contract(case)
        import slack_list_own_edit_runs as oracle
        return oracle.STAGES,oracle.validate
    source=inspect.getsource(base['run_fixture'])
    edits={
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'):":
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage','own-edit-runs'):",
        "controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))":
        "controller=_runs_storage(case)",
        "        evidence.update(ok=True, closedLogRevalidated=True)":
        "        if case == 'own-edit-runs':\n            _runs_closed(storage.closed_edit_snapshot(native)['snapshot'],evidence['native'])\n        evidence.update(ok=True, closedLogRevalidated=True)"}
    for old,new in edits.items():
        if source.count(old)!=1:raise RuntimeError('Original closed harness boundary changed')
        source=source.replace(old,new)
    base.update(case_contract=contract,NativeFixture=RunsFixture,_runs_storage=storage_controller,_runs_closed=closed)
    exec(compile(source,'<isolated original rich-runs owned harness>','exec'),base)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('slack-list','slack-list-narrow'))
    parser.add_argument('--case',choices=(CASE,),default=CASE)
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve(),args.case)


if __name__=='__main__':main()
