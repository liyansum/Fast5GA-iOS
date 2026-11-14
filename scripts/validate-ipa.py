#!/usr/bin/env python3
import sys,plistlib,hashlib,json,subprocess
from pathlib import Path
app=Path(sys.argv[1]); p=plistlib.loads((app/'Info.plist').read_bytes())
assert p['CFBundleIdentifier']=='com.liyansum.fast5ga'
assert p['CFBundleExecutable']=='Fast5GA'
binary=app/p['CFBundleExecutable']; assert binary.stat().st_size>0
assert binary.stat().st_mode & 0o111
assert not (app/'Resources').exists(), 'Resources changes CFBundle layout detection'
assert p['CFBundleShortVersionString']=='1.0.1'
resources=app/'CarrierPayloads'
for name,entry in json.loads((resources/'payloads.json').read_text()).items():
    b=(resources/name).read_bytes()
    assert len(b)==entry['size'] and hashlib.sha384(b).hexdigest()==entry['sha384'],name
symbols=subprocess.check_output(['nm','-g',str(binary)],text=True)
for symbol in ['_fg_run','_fg_pair_cancel','_al_pairing_run_host']:
    assert symbol in symbols,symbol
assert not list(app.rglob('*.rppairing'))
print('PASS: app metadata, bundled payloads and native operation symbols')
