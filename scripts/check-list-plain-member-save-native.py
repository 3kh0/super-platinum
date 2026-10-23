#!/usr/bin/env python3
"""Closed local member/group plain own-edit cases through the original owned harness."""
import runpy
from pathlib import Path
CASES=('own-edit-plain-member',)

def harness():
    module=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
    # All functions in the loaded isolated module share their own globals.
    namespace=module['harness'].__globals__
    namespace['CASES']=CASES
    ordinary=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
    controller=ordinary['storage_controller']('own-edit-plain')
    original=controller['StorageControl']
    class CustomStorage(original):
        def __init__(self,directory,expected,*,case):
            if case not in CASES:raise RuntimeError('Unexpected member storage case')
            super().__init__(directory,expected,case='own-edit-plain')
            self.evidence['configuredCase']=case
    controller['StorageControl']=CustomStorage
    def storage(case):
        if case not in CASES:raise RuntimeError('Unexpected member storage case')
        return controller
    namespace['storage_controller']=storage
    base=module['harness']()
    import slack_list_plain_member_save as oracle
    original_fixture=base['NativeFixture']
    class CustomFixture(original_fixture):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') not in CASES or 'SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_MEMBER_CHECK' in checks:
                raise RuntimeError('Unexpected member fixture')
            checks['SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_MEMBER_CHECK']='1'
            super().__init__(fixture,log,binary,checks,**kwargs)
    base['NativeFixture']=CustomFixture
    old=base['case_contract']
    base['case_contract']=lambda case:(oracle.STAGES[case],oracle.validator(case)) if case in CASES else old(case)
    base['_plain_closed']=lambda snapshot,receipt:oracle.validate_closed(snapshot,receipt)
    # Exact original plain admission is private source text, not global case flags.
    import inspect
    source=inspect.getsource(runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))['run_fixture'])
    edits={
      "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'):":
      "if case in ('own-edit-plain-member',):",
      "controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))":
      "controller=_plain_storage_controller(case)",
      "        evidence.update(ok=True, closedLogRevalidated=True)":
      "        _plain_closed(storage.closed_edit_snapshot(native)['snapshot'],evidence['native'])\n        evidence.update(ok=True, closedLogRevalidated=True)"}
    for old,new in edits.items():
        if source.count(old)!=1:raise RuntimeError('Original closed harness changed')
        source=source.replace(old,new)
    exec(compile(source,'<isolated original member owned harness>','exec'),base)
    return base

def main():
    import argparse
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
