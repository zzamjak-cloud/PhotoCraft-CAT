#!/bin/sh
# PhotoCAT macOS 업데이트 도우미 (mac_update.rs 가 임시 폴더에 써서 실행한다).
#
# 사용법: update.sh <앱 pid> <새 실행 파일> <교체할 실행 파일> <.app 번들> <버전> <실패 기록 파일>
#
# 앱이 끝나기를 기다렸다가, 서명 확인을 마친 새 실행 파일을 같은 폴더에 복사한 뒤 rename 으로
# 원자적으로 바꾼다. 실패하면 원본은 그대로 두고 이유를 기록 파일에 남긴다(다음 실행 때 상태
# 표시줄에 뜬다). 어느 경우든 앱을 다시 띄운다.
pid=$1
new=$2
exe=$3
app=$4
version=$5
log=$6
staged="$exe.update-new"

relaunch() {
  /usr/bin/open "$app" >/dev/null 2>&1 || true
}

fail() {
  rm -f "$staged"
  printf '%s\n' "$1" >"$log"
  relaunch
  exit 1
}

# 저장 안 한 작업은 앱이 닫히기 전에 처리했다. 10분 안에 닫히지 않으면 포기한다(앱은 계속 실행 중).
i=0
while kill -0 "$pid" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 3000 ]; then
    printf '%s\n' "the app did not close, so the update was not installed" >"$log"
    exit 1
  fi
  sleep 0.2
done
sleep 0.5

cp "$new" "$staged" || fail "couldn't copy the update into $app"
chmod 755 "$staged" || fail "couldn't make the update executable"
xattr -d com.apple.quarantine "$staged" 2>/dev/null
# 같은 폴더 안 rename 이라 원자적이다: 실패하면 원래 실행 파일이 남는다.
mv -f "$staged" "$exe" || fail "couldn't replace $exe"
rm -f "$new"

# 번들 표시 버전을 맞추고 ad-hoc 으로 다시 서명해 번들 서명과 실행 파일을 일치시킨다.
# 실패해도 새 실행 파일은 그대로 동작하므로 업데이트를 되돌리지 않는다.
plist="$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$plist" >/dev/null 2>&1
plutil -replace CFBundleVersion -string "$version" "$plist" >/dev/null 2>&1
codesign --force --deep --sign - --timestamp=none "$app" >/dev/null 2>&1
xattr -dr com.apple.quarantine "$app" 2>/dev/null

relaunch
