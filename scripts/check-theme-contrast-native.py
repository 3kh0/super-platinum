"""Computed-style evidence and real UI captures for four existing appearance fixtures."""
import argparse
import os
from pathlib import Path
from canvas_native import CanvasCheck, run
from native_fixture import DEFAULT_BINARY, ROOT

FIXTURES = ('appearance-countertop', 'appearance-blue-steel', 'appearance-paper-bag', 'appearance-custom-background')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/agent-ui/theme-contrast')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        spec = CanvasCheck(fixture, 'SUPER_PLATINUM_CONTRAST_CHECK',
                           ('sampledActual', 'sampledProbes', 'shell.ok', 'settings.ok', 'shell.probesRemoved', 'settings.probesRemoved'), {}, ('shell', 'settings'))
        run(spec, fixture, args.binary.resolve(), args.captures.resolve())

if __name__ == '__main__':
    main()
