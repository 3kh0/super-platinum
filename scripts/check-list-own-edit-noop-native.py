#!/usr/bin/env python3
"""Exact no-op List own-edit cases through the original closed native harness."""
import argparse
import inspect
from pathlib import Path
import runpy

FLAG='SUPER_PLATINUM_LIST_OWN_EDIT_NOOP_CHECK'
CASES=('own-edit-noop','own-edit-noop-storage')


def storage_controller(case):
    module=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))
    if case not in CASES:return module
    original=module['StorageControl']
    class NoopStorage(original):
        def __init__(self,directory,expected,*,case):
            if case not in CASES:raise RuntimeError('Unexpected no-op storage case')
            super().__init__(directory,expected,case='own-edit-storage')
            self.command_count=2 if case==CASES[1] else 0
            self.evidence.update(configuredCase=case,commandCount=self.command_count)
    module['StorageControl']=NoopStorage
    return module


def closed(snapshot,receipt):
    import slack_list_own_edit_noop as oracle
    oracle.validate_closed(snapshot,receipt)


def harness():
    loaded=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    base=loaded['run_fixture'].__globals__;original=base['NativeFixture'];old_contract=base['case_contract']
    class NoopFixture(original):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') not in CASES or FLAG in checks:
                raise RuntimeError('Unexpected List own-edit no-op mode')
            checks[FLAG]='1';super().__init__(fixture,log,binary,checks,**kwargs)
    def contract(case):
        if case not in CASES:return old_contract(case)
        import slack_list_own_edit_noop as oracle
        return oracle.STAGES[case],oracle.validator(case)
    # Extend ONLY exact storage admission and its final read-only validation in
    # a private copy. Original process, pointer, PNG, cleanup and log code remains.
    source=inspect.getsource(base['run_fixture'])
    edits={
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'):":
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage','own-edit-noop','own-edit-noop-storage'):",
        "controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))":
        "controller=_noop_storage_controller(case)",
        "        evidence.update(ok=True, closedLogRevalidated=True)":
        "        if case in ('own-edit-noop','own-edit-noop-storage'):\n            _noop_closed(storage.closed_edit_snapshot(native)['snapshot'],evidence['native'])\n        evidence.update(ok=True, closedLogRevalidated=True)"}
    for old,new in edits.items():
        if source.count(old)!=1:raise RuntimeError('Original closed harness boundary changed')
        source=source.replace(old,new)
    base.update(case_contract=contract,NativeFixture=NoopFixture,_noop_storage_controller=storage_controller,_noop_closed=closed)
    exec(compile(source,'<isolated original no-op owned harness>','exec'),base)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('slack-list','slack-list-narrow'))
    parser.add_argument('--case',choices=CASES,required=True)
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve(),args.case)


if __name__=='__main__':main()
