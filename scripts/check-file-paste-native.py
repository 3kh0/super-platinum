"""Synthetic WebView file-paste bridge proof. Never reads or writes the OS clipboard."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import sys
import tempfile
import time

from canvas_native import CanvasCheck, capture_stage, validate
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

SPEC = CanvasCheck('threads-file-paste', 'SUPER_PLATINUM_FILE_PASTE_CHECK',
                   ('noSend', 'synthetic', 'osClipboardUntouched', 'exactRustAttachments',
                    'exactFileBytes', 'outsideAndStaleUnattached', 'originalNamesPreserved'), {'fileCount': 2}, ('channel', 'thread'))
EXPECTED = (('日本語 🦊.txt', bytes([83, 80, 0, 10, 255])),
            ('résumé 🎹.txt', bytes([84, 72, 82, 69, 65, 68, 0, 254])))


def resolved_path(path):
    resolved = path.resolve(strict=True)
    # Rust canonicalize uses the Windows verbatim namespace. Compare the same
    # physical absolute spelling when the runner's mkdtemp path lacks it.
    if os.name == 'nt':
        value = str(resolved)
        if value.startswith('\\\\?\\UNC\\'):
            value = '\\\\' + value[8:]
        elif value.startswith('\\\\?\\'):
            value = value[4:]
        resolved = Path(value)
    return resolved


def verify_files(root, receipt):
    root = resolved_path(root)
    paths = receipt.get('paths')
    if not isinstance(paths, list) or len(paths) != 2 or any(not isinstance(p, str) for p in paths):
        raise RuntimeError('Expected two exact attachment paths')
    if receipt.get('names') != [name for name, _ in EXPECTED]:
        raise RuntimeError('Original attachment names changed')
    resolved = []
    evidence = []
    for raw, (name, expected) in zip(paths, EXPECTED):
        path = Path(raw)
        if not path.is_absolute() or path.is_symlink() or not path.is_file():
            raise RuntimeError('Attachment must be an absolute ordinary file')
        canonical = resolved_path(path)
        if canonical.parent != root or resolved_path(path.parent) != root:
            raise RuntimeError('Attachment escaped the private paste root')
        if path.name == name or not re.fullmatch(r'[0-9]+-[0-9]+-[0-9]+-pasted\.txt', path.name) or path.stat().st_size != len(expected):
            raise RuntimeError('Unexpected attachment name or size')
        data = path.read_bytes()
        if data != expected:
            raise RuntimeError('Attachment bytes changed')
        resolved.append(canonical)
        evidence.append({'physicalName': path.name, 'displayName': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    entries = list(root.iterdir())
    if len(set(resolved)) != 2 or len(entries) != 2 or {resolved_path(p) for p in entries} != set(resolved):
        raise RuntimeError('Duplicate or additional paste files')
    return evidence


def validate_final(evidence, fixture, receipts):
    evidence['native'] = receipts[0]
    if len(receipts) != 1:
        evidence['nativeReceipts'] = receipts
        raise RuntimeError('Expected exactly one final receipt')
    # Preserve the native assertion and report it before the secondary
    # missing-stage consequence.
    validate(SPEC, fixture, receipts[0])
    if len(evidence['stages']) != 2:
        raise RuntimeError('Expected exactly two captured stages')


def run(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {'fixture': fixture, 'binary': str(binary), 'platform': sys.platform,
                'ok': False, 'stages': [], 'inputKind': 'synthetic File/ClipboardEvent; not OS clipboard'}
    try:
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-') as private:
            private = Path(private).resolve()
            paste_root = private / 'paste'
            acknowledgments = private / 'ack'
            paste_root.mkdir()
            acknowledgments.mkdir()
            checks = {SPEC.flag: '1', 'SUPER_PLATINUM_FIXTURE_PASTE_ROOT': str(paste_root),
                      'SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR': str(acknowledgments)}
            with NativeFixture(fixture, captures / f'{fixture}.log', binary, checks) as native:
                evidence['pid'] = native.process.pid
                try:
                    deadline = time.monotonic() + 120
                    last_focus = 0
                    while True:
                        native.require_running()
                        stages = native.receipts('threads-file-paste-stage: ')
                        for stage in stages[len(evidence['stages']):]:
                            if (stage.get('synthetic') is not True or stage.get('noSend') is not True
                                    or stage.get('layout', {}).get('ok') is not True
                                    or stage.get('layout', {}).get('viewport') != expected_viewport(fixture)):
                                raise RuntimeError('Invalid synthetic paste stage')
                            evidence['stages'].append(capture_stage(native, SPEC, fixture, stage,
                                len(evidence['stages']), captures, acknowledgments))
                        receipts = native.receipts('threads-file-paste-check: ')
                        if receipts:
                            validate_final(evidence, fixture, receipts)
                            evidence['files'] = verify_files(paste_root, receipts[0])
                            screenshot = captures / f'{fixture}.png'
                            native.capture(screenshot)
                            evidence['screenshot'] = str(screenshot)
                            break
                        if time.monotonic() > deadline:
                            raise TimeoutError('Synthetic file paste did not finish')
                        if time.monotonic() - last_focus >= 1:
                            native.focus()
                            last_focus = time.monotonic()
                        time.sleep(.05)
                except BaseException:
                    try:
                        native.capture(captures / f'{fixture}-failure.png')
                    except (OSError, RuntimeError, ValueError):
                        pass
                    raise
            evidence['processExited'] = native.process.poll() is not None
            if not evidence['processExited']:
                raise RuntimeError('Owned file-paste fixture did not exit')
        evidence['privateDirectoryRemoved'] = not private.exists()
        if not evidence['privateDirectoryRemoved']:
            raise RuntimeError('Private file-paste directory was not removed')
        evidence['ok'] = True
    except BaseException as error:
        evidence['error'] = str(error)
        raise
    finally:
        output = captures / f'{fixture}.json'
        output.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'receipt': str(output), **evidence}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/agent-ui/file-paste')
    parser.add_argument('--fixture', action='append', choices=(SPEC.fixture, SPEC.fixture + '-narrow'))
    args = parser.parse_args()
    for fixture in args.fixture or (SPEC.fixture, SPEC.fixture + '-narrow'):
        run(fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
