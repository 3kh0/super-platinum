#!/usr/bin/env python3
"""Refresh the pinned, local-only PDF renderer. No npm install or package scripts."""
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

VERSION = '6.4.299'
INTEGRITY = 'sha512-AVl138zALtfaAPvADulE0PZThbYzCBS79nL4pOSL/6Sm/4AH5A21BD9VHt97OlCuzJuCpmeZtAtkinisF4Vb1g=='
URL = f'https://registry.npmjs.org/pdfjs-dist/-/pdfjs-dist-{VERSION}.tgz'
ROOT = Path(__file__).resolve().parents[1]
selected = []
with tempfile.TemporaryDirectory(prefix='super-platinum-pdfjs-') as temporary:
    archive = Path(temporary) / 'pdfjs.tgz'
    subprocess.run(['curl', '--fail', '--silent', '--show-error', URL, '-o', str(archive)], check=True)
    actual = 'sha512-' + base64.b64encode(hashlib.sha512(archive.read_bytes()).digest()).decode()
    if actual != INTEGRITY:
        raise SystemExit('PDF.js integrity mismatch')
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            name = member.name.removeprefix('package/')
            if not member.isfile() or '..' in Path(name).parts or Path(name).is_absolute():
                continue
            target = None
            if name in ['legacy/build/pdf.min.mjs', 'legacy/build/pdf.worker.min.mjs']:
                target = name.split('/')[-1].replace('.min', '')
            elif name == 'LICENSE' or name.startswith(('cmaps/', 'standard_fonts/', 'iccs/')):
                target = name
            elif name.startswith('wasm/') and 'quickjs' not in name:
                target = name
            if target:
                destination = ROOT / 'assets/pdfjs' / target
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(tar.extractfile(member).read())
                selected.append(target)
source = f'PDF.js {VERSION}\nMozilla and contributors; Apache-2.0 (see LICENSE and subdirectory licenses).\n{URL}\n{INTEGRITY}\nLegacy display/worker builds, CMaps, fonts, color profiles, and image decoders.\nPDF scripting sandbox and QuickJS are intentionally not bundled.\n'
(ROOT / 'assets/pdfjs/SOURCE.txt').write_text(source)
lines = ['// Generated from the pinned PDF.js distribution listed in assets/pdfjs/SOURCE.txt.', "pub fn asset(path: &str) -> Option<&'static [u8]> {", '    match path {']
for name in sorted(selected):
    if 'LICENSE' not in name:
        lines.append('        ' + json.dumps('/pdf/' + name) + ' => Some(include_bytes!("../../assets/pdfjs/' + name + '")),')
lines += ['        _ => None,', '    }', '}']
(ROOT / 'src/desktop/pdf_assets.rs').write_text('\n'.join(lines) + '\n')
subprocess.run(['cargo', 'fmt'], cwd=ROOT, check=True)
print(f'Vendored PDF.js {VERSION}, {len(selected)} files')
