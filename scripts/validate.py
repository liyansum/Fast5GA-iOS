#!/usr/bin/env python3
"""Offline parity check against the supplied batch script's frozen operation plan."""
import hashlib,json,plistlib,zipfile,io
from pathlib import Path
root=Path(__file__).resolve().parents[1]
r=root/'App/CarrierPayloads'
manifest=json.loads((r/'payloads.json').read_text())
for name,entry in manifest.items():
    b=(r/name).read_bytes()
    assert len(b)==entry['size'],name
    assert hashlib.sha384(b).hexdigest()==entry['sha384'],name
for name in ['unicom_72.0.1.plist','unicom_73.0.plist']:
    p=plistlib.loads((r/name).read_bytes())
    assert p['DerPriFileVersion']=='6.0.161'
    assert p['DataIndicatorOverrideForNRMmwave']=='NRCA'
    assert p['DerPriFileName']=='../'*6+'var/mobile/Library/Carrier Bundles/iPhone/Unicom_cn.bundle/overrides_D73_D74_D27_D28.der.pri'
    assert p['Show5GStandaloneSwitch'] is True and p['ShowVoNRSwitch'] is True
    assert 'DataIndicatorOverrideForLTE' not in p
raw=(r/'Unicom_cn_73.0_original.ipcc').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='54b8895eec8e815830d6587d6500fdd11a34c6ed4522b4490e74801435f845c6'
z=zipfile.ZipFile(io.BytesIO(raw)); assert z.testzip() is None
original=json.loads((r/'original_ipcc.json').read_text())
for name,entry in original['files'].items():
    b=z.read('Payload/Unicom_cn.bundle/'+name)
    assert len(b)==entry['size'] and hashlib.sha384(b).hexdigest()==entry['sha384'],name
info=plistlib.loads((root/'App/Info.plist').read_bytes())
assert '_remotepairing-pairable-host._tcp' in info['NSBonjourServices']
assert '_remotepairing._tcp' in info['NSBonjourServices']
assert info['CFBundleDisplayName']=='开启5GA'
print('PASS: all 9 payload hashes, original IPCC, Overlay fields and pairing metadata')
