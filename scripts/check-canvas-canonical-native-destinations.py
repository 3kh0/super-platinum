"""Closed original Canvas-comment links into actual native Canvas/List readers."""
import argparse
import os
from pathlib import Path
import canvas_native
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import canvas_canonical_native_destinations as oracle

SPEC = CanvasCheck(
    fixture='canvas-canonical-native-destinations',
    flag='SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK',
    required=('draftPreserved', 'freshTarget', 'staleRefused', 'noDuplicateDispatch'),
    counts={'requestCount': 5, 'resolverCount': 2}, stages=oracle.STAGES,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),))
_base = canvas_native.validate
_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    expected = oracle.CANVAS if '-canvas' in fixture else oracle.LIST
    oracle.require(fixture in oracle.FIXTURES and receipt.get('destinationId') == expected,
                   'Named native destination differs')
    oracle.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require(oracle.same([{k: v for k, v in s.items() if k != 'screenshot'}
                               for s in stages], terminal.get('stages')),
                   'Captured original native destinations changed')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path,
                   default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    p.add_argument('--captures', type=Path,
                   default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', ROOT / 'tmp/agent-ui')))
    p.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    args = p.parse_args()
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPEC, fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
