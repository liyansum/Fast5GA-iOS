#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
xcodegen generate
xcodebuild -project Fast5GA.xcodeproj -scheme Fast5GA -configuration Release \
  -derivedDataPath build/DerivedData -destination 'generic/platform=iOS' \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY='' \
  build 2>&1 | tee build/xcodebuild.log
app_path=build/DerivedData/Build/Products/Release-iphoneos/Fast5GA.app
test -d "$app_path"
mkdir -p build/Payload
cp -R "$app_path" build/Payload/Fast5GA.app
python3 scripts/validate-ipa.py build/Payload/Fast5GA.app
xcrun swift scripts/verify-bundle.swift build/Payload/Fast5GA.app
(cd build && /usr/bin/zip -qry Fast5GA.ipa Payload)
(cd build && shasum -a 256 Fast5GA.ipa > SHA256SUMS.txt)
