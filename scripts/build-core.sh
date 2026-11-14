#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
export IPHONEOS_DEPLOYMENT_TARGET=18.0
mkdir -p build
cargo +1.100.0 build --locked --release --target aarch64-apple-ios --manifest-path rust-core/Cargo.toml 2>&1 | tee build/rust.log
xcodebuild -create-xcframework \
  -library rust-core/target/aarch64-apple-ios/release/libairlift_ffi.a \
  -headers rust-core/include \
  -output AirliftFFI.xcframework
