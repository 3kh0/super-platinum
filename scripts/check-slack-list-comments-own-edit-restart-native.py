#!/usr/bin/env python3
"""Two closed native processes, transferring only exact persisted SQLite bytes.

This proves fresh-process recovery, not an OS crash or an in-place restart.
It never builds; the caller supplies the already reviewed build artifacts.
"""
import argparse
import json
from pathlib import Path
import runpy
import sys

ROOT = Path(__file__).resolve().parents[1]
C = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
S = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_restart_storage.py')))
require = S['require']


def build_proof(binary, result_path):
    result_path = Path(result_path)
    S['plain'](result_path)
    result = S['decoded'](result_path.read_text(encoding='utf-8'))
    require(result.get('exitCode') == 0 and type(result.get('exitCode')) is int
        and result.get('inputsUnchanged') is True and result.get('binarySha256') == S['sha'](binary),
        'Restart requires a successful unchanged-source build for this exact binary')
    manifest = result_path.with_name('input-hashes.json')
    require(S['plain'](manifest).st_size <= 8 * 1024 * 1024, 'Build manifest exceeds bounds')
    files = S['decoded'](manifest.read_text(encoding='utf-8'))
    require(isinstance(files, dict) and files and all(isinstance(p, str) and isinstance(h, str) for p, h in files.items()), 'Malformed build source manifest')
    for name, digest in files.items():
        relative = Path(name)
        require(not relative.is_absolute() and '..' not in relative.parts, 'Foreign build source input')
        path = ROOT / relative
        S['plain'](path)
        require(S['sha'](path) == digest, 'Compiled source changed before/between restart phases: ' + name)
    return dict(result=str(result_path), resultSha256=S['sha'](result_path), manifest=str(manifest),
                manifestSha256=S['sha'](manifest), binarySha256=S['sha'](binary), inputCount=len(files))


def run_pair(mode, fixture, binary, captures, *, build_result=None, operation='edit'):
    require(operation in ('edit','delete') and mode in (('unsent','unknown') if operation=='edit' else ('pending','unknown'))
        and fixture in C['FIXTURES'], 'Unknown restart operation/mode/fixture')
    prefix='own-'+operation+'-restart-'
    binary, captures = Path(binary).resolve(), Path(captures).resolve()
    require(not captures.exists(), 'Use a fresh restart capture directory')
    captures.mkdir(parents=True)
    build_result = Path(build_result or binary.with_name('result.json')).resolve()
    evidence = dict(ok=False, mode=mode, fixture=fixture, binary=str(binary), phases=[],
        limitations=['Fresh-process recovery from an exact copy of closed persisted database bytes; not an OS-crash or in-place-restart test.',
                     'Native placement/pointer checks are sampled; original PNG visual inspection is separately required.'])
    try:
        evidence['build'] = build_proof(binary, build_result)
        first_dir, second_dir = captures / 'exercise', captures / 'restore'
        first = C['run_fixture'](fixture, binary, first_dir, prefix + mode + '-exercise')
        evidence['phases'].append(str(first_dir / f'{fixture}.json'))
        require(first.get('ok') is True and first.get('processExited') is True and first.get('closedLogRevalidated') is True
            and first.get('cleanupErrors') == [], 'First process must close before restore is launched')
        original = S['database'](Path(first['readyDirectory']) / 'comments.sqlite', operation=operation)
        evidence['closedExerciseDatabase'] = original
        require(build_proof(binary, build_result) == evidence['build'], 'Original build changed after exercise')
        second = C['run_fixture'](fixture, binary, second_dir, prefix + mode + '-restore',
                                  restart_from=first_dir / f'{fixture}.json')
        evidence['phases'].append(str(second_dir / f'{fixture}.json'))
        require(second.get('ok') is True and second.get('processExited') is True and second.get('closedLogRevalidated') is True
            and second.get('cleanupErrors') == [], 'Restore process did not close cleanly')
        require(first['pid'] != second['pid'] and first['nonce'] != second['nonce']
            and first['readyDirectory'] != second['readyDirectory'] and first['binarySha256'] == second['binarySha256'],
            'Restore reused original process/readiness or changed executable')
        if 'processCreationFiletime' in first:
            require(second['processCreationFiletime'] > first['exitIdentity']['exit_filetime'],
                    'Restore process predates original confirmed exit')
        transfer = second.get('restartTransfer', {})
        require(transfer.get('source') == original and transfer.get('destination', {}).get('sha256') == original['sha256'],
                'Restore did not start from exact closed exercise database')
        require(S['database'](Path(original['path']), operation=operation) == original, 'Restore modified original exercise database')
        restored = S['database'](Path(second['readyDirectory']) / 'comments.sqlite', operation=operation)
        evidence['closedRestoreDatabase'] = restored
        oracle = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_'+operation+'_restart.py')))
        oracle['validate_pair'](mode, first['native'], second['native'], original, transfer['destination'], restored)
        require(build_proof(binary, build_result) == evidence['build'], 'Original build changed during restore')
        evidence.update(ok=True, firstProcessClosedBeforeSecond=True, sameBinaryAndSource=True,
                        persistedDatabaseRetained=True, freshProcessRecovery=True,
                        restartTransfer=transfer, visualInspectionPending=True)
        return evidence
    except BaseException as error:
        evidence['error'] = str(error)
        raise
    finally:
        path = captures / 'restart.json'
        path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(dict(receipt=str(path), **evidence)), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--build-result', type=Path)
    parser.add_argument('--captures', required=True, type=Path)
    parser.add_argument('--fixture', action='append', choices=C['FIXTURES'])
    parser.add_argument('--mode', required=True, choices=('unsent', 'unknown'))
    args = parser.parse_args()
    for fixture in args.fixture or C['FIXTURES']:
        run_pair(args.mode, fixture, args.binary, args.captures.resolve() / fixture, build_result=args.build_result)


if __name__ == '__main__':
    main()
