#!/usr/bin/env python3
"""Closed standard-name/Unicode own-edit case through the original owned harness."""
import runpy
from pathlib import Path
CASE='own-edit-plain-emoji'

def harness():
    module=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
    # All functions in the loaded isolated module share their own globals.
    namespace=module['harness'].__globals__
    namespace['CASES']=(CASE,)
    ordinary=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-plain-native.py')))
    controller=ordinary['storage_controller']('own-edit-plain')
    original=controller['StorageControl']
    class EmojiStorage(original):
        def __init__(self,directory,expected,*,case):
            if case!=CASE:raise RuntimeError('Unexpected emoji storage case')
            super().__init__(directory,expected,case='own-edit-plain')
            self.evidence['configuredCase']=CASE
    controller['StorageControl']=EmojiStorage
    def storage(case):
        if case!=CASE:raise RuntimeError('Unexpected emoji storage case')
        return controller
    namespace['storage_controller']=storage
    base=module['harness']()
    import slack_list_own_edit_plain_emoji as oracle
    old=base['case_contract']
    base['case_contract']=lambda case:(oracle.STAGES[CASE],oracle.validator(CASE)) if case==CASE else old(case)
    base['_plain_closed']=lambda snapshot,receipt:oracle.validate_closed(snapshot,receipt)
    # Exact original plain admission is private source text, not global case flags.
    import inspect
    source=inspect.getsource(runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))['run_fixture'])
    edits={
      "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'):":
      "if case == 'own-edit-plain-emoji':",
      "controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))":
      "controller=_plain_storage_controller(case)",
      "        evidence.update(ok=True, closedLogRevalidated=True)":
      "        _plain_closed(storage.closed_edit_snapshot(native)['snapshot'],evidence['native'])\n        evidence.update(ok=True, closedLogRevalidated=True)"}
    for old,new in edits.items():
        if source.count(old)!=1:raise RuntimeError('Original closed harness changed')
        source=source.replace(old,new)
    exec(compile(source,'<isolated original emoji owned harness>','exec'),base)
    return base

def main():
    import argparse
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('slack-list','slack-list-narrow'))
    parser.add_argument('--case',choices=(CASE,),required=True)
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve(),args.case)

if __name__=='__main__':main()
