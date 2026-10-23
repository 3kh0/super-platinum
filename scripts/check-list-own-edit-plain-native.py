#!/usr/bin/env python3
"""Exact plain-mode List own-edit cases through the original closed native harness."""
import argparse
import inspect
import textwrap
from pathlib import Path
import runpy

FLAG='SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK'
CASES=('own-edit-plain','own-edit-plain-storage')


def plain_inventory_shape(value):
    base={'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}
    if not isinstance(value,dict):return False
    if set(value)==base:return value.get('seed') is None and value.get('blocks')==[] and value.get('attempt') is None
    if set(value)!=base|{'plain_delta'} or value.get('blocks')!=[]:return False
    delta=value.get('plain_delta');seed=value.get('seed')
    if not isinstance(delta,dict) or set(delta)!={'version','ops'} or type(delta['version']) is not int or delta['version']!=1:return False
    if not isinstance(seed,dict) or not isinstance(seed.get('plain'),dict):return False
    # This named fixture deliberately exercises ordinary text only. Do not turn
    # the old bounded storage reader into an arbitrary semantic Delta decoder.
    ops=delta.get('ops')
    return isinstance(ops,list) and 0<len(ops)<=4096 and all(isinstance(op,dict) and set(op)=={'insert'} and isinstance(op['insert'],str) for op in ops)


def storage_controller(case):
    module=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))
    if case not in CASES:return module
    original=module['StorageControl']
    source=textwrap.dedent(inspect.getsource(original._snapshot))
    old="isinstance(value, dict) and set(value) == {'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}"
    if source.count(old)!=1:raise RuntimeError('Original storage inventory boundary changed')
    source=source.replace(old,'_plain_inventory_shape(value)')
    namespace=dict(original._snapshot.__globals__,_plain_inventory_shape=plain_inventory_shape)
    exec(compile(source,'<isolated original plain inventory reader>','exec'),namespace)
    class PlainStorage(original):
        _snapshot=namespace['_snapshot']
        def __init__(self,directory,expected,*,case):
            if case not in CASES:raise RuntimeError('Unexpected plain-mode storage case')
            super().__init__(directory,expected,case='own-edit-storage')
            self.command_count=2 if case==CASES[1] else 0
            self.evidence.update(configuredCase=case,commandCount=self.command_count)
    module['StorageControl']=PlainStorage
    return module


def closed(snapshot,receipt):
    import slack_list_own_edit_plain as oracle
    oracle.validate_closed(snapshot,receipt)


def harness():
    loaded=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    base=loaded['run_fixture'].__globals__;original=base['NativeFixture'];old_contract=base['case_contract']
    class PlainFixture(original):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') not in CASES or FLAG in checks:
                raise RuntimeError('Unexpected List own-edit plain-mode mode')
            checks[FLAG]='1';super().__init__(fixture,log,binary,checks,**kwargs)
    def contract(case):
        if case not in CASES:return old_contract(case)
        import slack_list_own_edit_plain as oracle
        return oracle.STAGES[case],oracle.validator(case)
    # Extend ONLY exact storage admission and its final read-only validation in
    # a private copy. Original process, pointer, PNG, cleanup and log code remains.
    source=inspect.getsource(base['run_fixture'])
    edits={
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage'):":
        "if case in ('storage-recovery','file-navigation-custody','own-edit-storage','own-delete-storage','own-edit-plain','own-edit-plain-storage'):",
        "controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))":
        "controller=_plain_storage_controller(case)",
        "        evidence.update(ok=True, closedLogRevalidated=True)":
        "        if case in ('own-edit-plain','own-edit-plain-storage'):\n            _plain_closed(storage.closed_edit_snapshot(native)['snapshot'],evidence['native'])\n        evidence.update(ok=True, closedLogRevalidated=True)"}
    for old,new in edits.items():
        if source.count(old)!=1:raise RuntimeError('Original closed harness boundary changed')
        source=source.replace(old,new)
    base.update(case_contract=contract,NativeFixture=PlainFixture,_plain_storage_controller=storage_controller,_plain_closed=closed)
    exec(compile(source,'<isolated original plain-mode owned harness>','exec'),base)
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
