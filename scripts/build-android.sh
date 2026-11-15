#!/usr/bin/env bash
# Build the Android APK (#354) into dist/android/friendslop.apk (or $1).
#   nix develop .#android -c scripts/build-android.sh
# The shell provides Rust with the Android targets, the SDK, NDK and
# cargo-apk. Install with `adb install -r dist/android/friendslop.apk`.
# Release builds are signed with CARGO_APK_RELEASE_KEYSTORE(_PASSWORD) when
# set, else with a throwaway key made on first build (fine for sideloading;
# a store upload needs a real one).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-dist/android}"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
TARGET="${ANDROID_TARGET:-aarch64-linux-android}"
PROFILE="${ANDROID_PROFILE:-release}"

if [[ -z "${CARGO_APK_RELEASE_KEYSTORE:-}" ]]; then
  key="$TARGET_DIR/friendslop-local.keystore"
  if [[ ! -f "$key" ]]; then
    mkdir -p "$TARGET_DIR"
    keytool -genkeypair -keystore "$key" -storepass android -alias androiddebugkey \
      -keypass android -keyalg RSA -keysize 2048 -validity 10000 \
      -dname "CN=friendSLOP local" >/dev/null
  fi
  export CARGO_APK_RELEASE_KEYSTORE="$key" CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android
fi

flag=()
[[ "$PROFILE" == release ]] && flag=(--release)
cargo apk build -p friendslop-android --lib --target "$TARGET" "${flag[@]}"

mkdir -p "$OUT"
cp "$TARGET_DIR/$PROFILE/apk/friendslop.apk" "$OUT/friendslop.apk"
echo "$OUT/friendslop.apk"
