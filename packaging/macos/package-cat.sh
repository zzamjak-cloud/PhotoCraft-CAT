#!/usr/bin/env bash
# PhotoCAT 포크의 macOS(Apple Silicon) 릴리스 산출물을 만든다. Apple 인증서가 없으므로
# ad-hoc 서명(codesign -s -)만 하고 공증하지 않는다.
#
#   $DIST/photocraft-cat-<버전>-macos-aarch64.dmg     "PhotoCAT.app" 을 담은 설치용 DMG
#   $DIST/photocraft-cat-<버전>-macos-aarch64.tar.gz  앱 자체 업데이트용 (photocraft-cat 바이너리 1개)
#
# 사용법: packaging/macos/package-cat.sh [--skip-build]
# 버전: PHOTOCRAFT_VERSION (없으면 Cargo.toml 의 워크스페이스 버전). 앱이 보고하는 버전도
#       PHOTOCRAFT_OVERRIDE_VERSION 으로 같은 값을 넣어 빌드한다.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

SKIP_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# craft-update 의 UPDATE_APP.id 와 같아야 한다 (crates/ui-egui/src/lib.rs).
APP_ID="photocraft-cat"
APP_NAME="PhotoCAT"
# ai.storyteller.<id> 와 달라야 한다: 같으면 craft-update 가 공식 팀 Developer ID 서명을 요구하는
# DMG 업데이트 경로를 택해 ad-hoc 빌드는 업데이트할 수 없다. 자체 업데이트는 포크 전용
# crates/ui-egui/src/mac_update.rs 가 서명된 tarball 로 번들 안 실행 파일을 교체한다.
BUNDLE_ID="com.zzamjak.photocraft-cat"

export MACOSX_DEPLOYMENT_TARGET=11.0
export PHOTOCRAFT_OVERRIDE_VERSION="$VERSION"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package-cat"
APP="$WORK/$APP_NAME.app"
STEM="$APP_ID-$VERSION-macos-aarch64"
DMG="$DIST/$STEM.dmg"
TARBALL="$DIST/$STEM.tar.gz"

echo "==> $APP_NAME $VERSION for macOS (aarch64), ad-hoc signed"

if [[ "$SKIP_BUILD" = 0 ]]; then
  (cd "$ROOT" && cargo build --release --locked -p photocraft)
fi
BIN="$CARGO_TARGET_DIR/release/photocraft"
if [[ "$(lipo -archs "$BIN")" != *arm64* ]]; then
  echo "error: $BIN is not an arm64 binary (build on Apple Silicon)" >&2
  exit 1
fi

# ---- .app 번들 ---------------------------------------------------------------------------------
rm -rf "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/PhotoCraft"
cp "$ROOT/assets/app-icon/photocraft.icns" "$APP/Contents/Resources/PhotoCraft.icns"
cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/NOTICE" "$ROOT/ATTRIBUTION.md" "$APP/Contents/Resources/"
if [[ -n "${CRAFT_FONTS_DIR:-}" ]]; then
  mkdir -p "$APP/Contents/Resources/Licenses"
  copy_font_licences "$APP/Contents/Resources/Licenses"
fi
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  -e "s/@BUILD_SHA@/${PHOTOCRAFT_BUILD_SHA:-unknown}/g" \
  "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$APP/Contents/Info.plist"
plutil -replace CFBundleName -string "$APP_NAME" "$APP/Contents/Info.plist"
plutil -replace CFBundleDisplayName -string "$APP_NAME" "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"

codesign --force --deep --sign - --timestamp=none "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

# ---- 자체 업데이트용 tarball ----------------------------------------------------------------------
# mac_update.rs 가 tarball 에서 이름이 <id> 인 파일을 꺼내 번들의 Contents/MacOS/PhotoCraft 를 교체한다.
# 릴리스 워크플로가 tarball 을 Ed25519 로 서명해 .tar.gz.sig 를 함께 올린다.
TAR_DIR="$WORK/tar"
mkdir -p "$TAR_DIR"
cp "$APP/Contents/MacOS/PhotoCraft" "$TAR_DIR/$APP_ID"
rm -f "$TARBALL"
tar -C "$TAR_DIR" -czf "$TARBALL" "$APP_ID"

# ---- DMG ---------------------------------------------------------------------------------------
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/$APP_NAME.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG" "$WORK/raw.dmg"
hdiutil makehybrid -hfs -hfs-volume-name "$APP_NAME" -hfs-openfolder "$STAGE" -o "$WORK/raw.dmg" "$STAGE"
hdiutil convert "$WORK/raw.dmg" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$WORK/raw.dmg"

echo "==> done"
ls -lh "$DMG" "$TARBALL"
