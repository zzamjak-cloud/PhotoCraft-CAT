#!/usr/bin/env bash
# 원본 PhotoCraft(storytold/photocraft) 변경을 포크로 가져올 때 놓치기 쉬운 것을 점검한다.
#
#   scripts/upstream-sync.sh status            원본 새 릴리스·커밋 수, main 의 구조 변경 후보
#   scripts/upstream-sync.sh check <태그>       병합 전 위험 보고 (예: v0.24.0)
#   scripts/upstream-sync.sh verify            병합 중/후 작업 트리 검사 (마커·버전·미추적·lock·포크 기능)
#   scripts/upstream-sync.sh test              외부 빌드 입력(craft-i18n·craft-fonts)을 받아 회귀 테스트 실행
#
# 절차는 CLAUDE.md "원본 병합" 절. 포크 기능 목록은 scripts/upstream-sync-anchors.tsv.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
UPSTREAM_URL=https://github.com/storytold/photocraft.git
FORK_VERSION=0.19.0
ANCHORS="$ROOT/scripts/upstream-sync-anchors.tsv"
fail=0
warn() { printf '  ⚠ %s\n' "$*"; }
bad() { printf '  ✖ %s\n' "$*"; fail=1; }
ok() { printf '  ✔ %s\n' "$*"; }
section() { printf '\n== %s\n' "$*"; }

ensure_remote() {
  git remote get-url upstream >/dev/null 2>&1 || git remote add upstream "$UPSTREAM_URL"
  # 원본 태그가 포크 태그(vX.Y.Z, 푸시하면 Release 가 돈다)와 섞이지 않게 별도 공간으로 받는다.
  git config remote.upstream.tagOpt --no-tags
  git config --get-all remote.upstream.fetch | grep -qx '+refs/tags/\*:refs/upstream-tags/\*' ||
    git config --add remote.upstream.fetch '+refs/tags/*:refs/upstream-tags/*'
  git fetch --quiet upstream
}

# 포크에 이미 들어온 가장 최근 원본 태그.
merged_tag() {
  git for-each-ref --sort=-v:refname --format='%(refname:strip=2)' refs/upstream-tags |
    while read -r t; do git merge-base --is-ancestor "refs/upstream-tags/$t" HEAD && { echo "$t"; break; }; done
}

cmd_status() {
  ensure_remote
  local base latest
  base="$(merged_tag)"
  latest="$(git for-each-ref --count=1 --sort=-v:refname --format='%(refname:strip=2)' refs/upstream-tags)"
  section "원본 릴리스"
  echo "  포크에 들어온 태그: ${base:-없음} / 원본 최신 태그: $latest"
  git for-each-ref --sort=-v:refname --format='%(refname:strip=2)' refs/upstream-tags |
    while read -r t; do [[ "$t" == "$base" ]] && break; echo "  새 릴리스: $t ($(git log -1 --format=%ad --date=short "refs/upstream-tags/$t"))"; done
  echo "  HEAD..$latest: $(git rev-list --count --no-merges "HEAD..refs/upstream-tags/$latest") 커밋"
  echo "  $latest..upstream/main: $(git rev-list --count --no-merges "refs/upstream-tags/$latest..upstream/main") 커밋 (아직 릴리스 안 됨)"
  section "릴리스 이후 main 의 대규모 커밋 (파일 300개 이상 — 구조 변경 후보)"
  git log --no-merges --format='%h' "refs/upstream-tags/$latest..upstream/main" | while read -r c; do
    n=$(git show --format= --name-only "$c" | wc -l)
    (( n >= 300 )) && echo "  $c $(git log -1 --format='%ad %s' --date=short "$c") [$n 파일]"
  done
  return 0
}

cmd_check() {
  local ref="${1:?병합할 원본 태그를 지정 (예: v0.24.0)}"
  ensure_remote
  local target="refs/upstream-tags/$ref"
  git rev-parse -q --verify "$target" >/dev/null || target="$ref"
  local mb
  mb="$(git merge-base HEAD "$target")"
  local fork_files up_deleted
  fork_files="$(git diff --name-only "$mb" HEAD | sort)"

  section "1. 포크가 고친 파일을 원본이 지우거나 옮김 (포크 내용이 조용히 사라질 수 있음)"
  up_deleted="$(git diff --name-only --diff-filter=DR -M "$mb" "$target" | sort)"
  comm -12 <(echo "$fork_files") <(echo "$up_deleted") | sed 's/^/  ✖ /' | head -50
  git diff --name-status --diff-filter=R -M "$mb" "$target" | awk '{print $2" -> "$3}' |
    while read -r from _ to; do echo "$fork_files" | grep -qx "$from" && echo "  → 이동: $from -> $to"; done || true

  section "2. 새 외부 빌드 입력 (환경 변수·고정 rev 파일·storytold 저장소)"
  git diff "$mb" "$target" -- '*build.rs' | grep -oE '^\+.*(env::var(_os)?\("[A-Z0-9_]+"\)|rerun-if-env-changed=[A-Z0-9_]+)' | grep -oE '[A-Z][A-Z0-9_]{3,}' | sort -u | sed 's/^/  env: /' || true
  git diff --name-only --diff-filter=A "$mb" "$target" -- '*.rev' | sed 's/^/  rev 파일: /'
  comm -13 <(git grep -hoE 'storytold/[A-Za-z0-9_.-]+' "$mb" -- Cargo.toml .github | sed 's/\.git$//' | sort -u) \
    <(git grep -hoE 'storytold/[A-Za-z0-9_.-]+' "$target" -- Cargo.toml .github | sed 's/\.git$//' | sort -u) | sed 's/^/  새 저장소: /'

  section "3. 원본 워크플로 변경 (포크는 자체 release.yml·ci.yml 유지 — 필요한 단계만 옮긴다)"
  git diff --stat "$mb" "$target" -- .github/workflows | sed 's/^/  /' | tail -15
  git diff --name-only --diff-filter=A "$mb" "$target" -- .github/workflows | sed 's/^/  ✖ 새 워크플로(포크에서 돌게 됨, 필요 없으면 지운다): /'
  local up_fonts fork_fonts
  up_fonts="$(git show "$target:.github/workflows/release.yml" 2>/dev/null | grep -oE 'craft-fonts-ref: [0-9a-f]{40}' | head -1 | awk '{print $2}')"
  fork_fonts="$(grep -oE 'CRAFT_FONTS_REF: [0-9a-f]{40}' .github/workflows/release.yml | awk '{print $2}')"
  if [[ -n "$up_fonts" && "$up_fonts" != "$fork_fonts" ]]; then bad "craft-fonts-ref 가 바뀜: 원본 $up_fonts / 포크 $fork_fonts (release.yml CRAFT_FONTS_REF 갱신)"; fi

  section "4. 화면 문구·브랜딩 (PhotoCraft 이름, ArtCraft 로고·사이트)"
  git diff "$mb" "$target" -- '*.rs' | grep -E '^\+' | grep -E '(tl!|tr\(|label|title).*"[^"]*PhotoCraft' | grep -vE '^\+\s*//|assert' | head -15 | sed 's/^/  /' || true
  git diff --name-only --diff-filter=AM "$mb" "$target" | grep -iE 'artcraft|logo' | sed 's/^/  ⚠ 로고 후보: /' || true
  git diff "$mb" "$target" | grep -E '^\+' | grep -oiE 'https?://[a-z0-9.]*artcraft[a-z0-9./_-]*' | grep -v 'ns.getartcraft.com' | sort -u | sed 's/^/  ⚠ 링크: /' || true

  section "5. 포크 기능 파일에 들어오는 원본 변경량"
  cut -f1 "$ANCHORS" | grep -v '^#' | sort -u | while read -r f; do
    [[ -z "$f" ]] && continue
    # 포크에만 있는 파일(package-cat.sh, i18n-cat 등)은 원본에 없는 게 정상이다.
    git cat-file -e "$mb:$f" 2>/dev/null || continue
    git cat-file -e "$target:$f" 2>/dev/null || { bad "$f 가 원본에서 사라짐"; continue; }
    s="$(git diff --shortstat "$mb" "$target" -- "$f")"
    [[ -n "$s" ]] && echo "  $f:$s"
  done
  echo
  echo "충돌 예상: $(comm -12 <(echo "$fork_files") <(git diff --name-only "$mb" "$target" | sort) | wc -l | tr -d ' ') 파일 (양쪽 모두 수정)"
  return $fail
}

cmd_verify() {
  section "충돌 마커"
  local m
  m="$(git grep -lE '^(<<<<<<<|>>>>>>>) ' -- . ':!*.md' 2>/dev/null || true)"
  [[ -z "$m" ]] && ok "없음" || { echo "$m" | sed 's/^/  /'; bad "충돌 마커 남음"; }
  local u
  u="$(git diff --name-only --diff-filter=U)"
  [[ -z "$u" ]] || bad "미해결 경로: $u"

  section "버전 (Cargo.toml 은 $FORK_VERSION 유지, 버전은 태그가 정함)"
  grep -qE "^version = \"$FORK_VERSION\"" Cargo.toml && ok "$FORK_VERSION" || bad "Cargo.toml 버전이 $FORK_VERSION 이 아님"
  if git grep -qE "^name = \"photocraft\"" -- Cargo.lock 2>/dev/null; then
    grep -A1 '^name = "photocraft-engine"' Cargo.lock | grep -q "version = \"$FORK_VERSION\"" && ok "Cargo.lock 워크스페이스 버전" || bad "Cargo.lock 이 원본 버전: cargo 로 다시 맞춘다"
  fi

  section "stage 누락 (병합 중일 때)"
  if git rev-parse -q --verify MERGE_HEAD >/dev/null; then
    git ls-files --others --exclude-standard -- crates apps packaging scripts .github | sed 's/^/  ✖ 미추적: /'
    [[ -z "$(git ls-files --others --exclude-standard -- crates apps packaging scripts .github)" ]] || fail=1
    git diff --name-only | sed 's/^/  ⚠ 수정 후 stage 안 됨: /'
  else
    ok "병합 중 아님"
  fi

  section "포크 기능 ($ANCHORS)"
  while IFS=$'\t' read -r f pat what; do
    [[ -z "$f" || "$f" == \#* ]] && continue
    if [[ ! -f "$f" ]]; then bad "$what: $f 없음"
    elif grep -qE -- "$pat" "$f"; then :
    else bad "$what: $f 에 /$pat/ 없음"
    fi
  done <"$ANCHORS"
  ((fail)) || ok "모두 있음"

  section "번역 (i18n-cat 오버레이 · craft-i18n.rev)"
  [[ -f craft-i18n.rev ]] && ok "craft-i18n.rev $(cut -c1-12 craft-i18n.rev)" || warn "craft-i18n.rev 없음: build.rs 번역 방식이 바뀌었는지 확인"
  grep -q 'cat_catalog' crates/ui-egui/build.rs && ok "build.rs cat_catalog 연결" || bad "build.rs 에 cat_catalog 없음: 포크 번역·PhotoCAT 이름 치환이 빠짐"
  grep -q 'CRAFT_I18N_DIR' .github/workflows/release.yml && ok "release.yml 번역 체크아웃" || bad "release.yml 에 CRAFT_I18N_DIR 없음 (영어 전용 빌드)"
  return $fail
}

# 외부 빌드 입력을 rev 대로 받아 둔다. 저장소 루트의 craft-i18n/, craft-fonts/ (gitignore).
checkout_pinned() {
  local repo="$1" rev="$2" dir="$ROOT/$1"
  [[ -d "$dir/.git" ]] || git clone --quiet "https://github.com/storytold/$repo.git" "$dir"
  git -C "$dir" cat-file -e "$rev^{commit}" 2>/dev/null || git -C "$dir" fetch --quiet origin "$rev"
  git -C "$dir" checkout --quiet --detach "$rev"
}

cmd_test() {
  checkout_pinned craft-i18n "$(tr -d '[:space:]' <craft-i18n.rev)"
  checkout_pinned craft-fonts "$(grep -oE 'CRAFT_FONTS_REF: [0-9a-f]{40}' .github/workflows/release.yml | awk '{print $2}')"
  # 폰트·번역이 없으면 i18n 라이브 테스트가 글리프 없음으로 실패하거나 건너뛴다.
  export CRAFT_I18N_DIR="$ROOT/craft-i18n" CRAFT_FONTS_DIR="$ROOT/craft-fonts" CRAFT_I18N_REQUIRED=1
  local f
  for f in cat:: dock update_app_tests i18n panel_docking layer_menu_ui vector_ui; do
    printf '%-18s ' "$f"
    cargo test -q -p photocraft-ui-egui --lib "$f" 2>&1 | grep -E '^test result|^error' | tail -1
  done
  printf '%-18s ' engine
  local skip=()
  # macOS(APFS)는 UTF-8 이 아닌 파일 이름을 거부한다. 원본 CI(Linux)용 테스트라 건너뛴다.
  [[ "$(uname)" == Darwin ]] && skip=(-- --skip read_file_within_reads_a_path_that_is_not_utf8)
  cargo test -q -p photocraft-engine --lib "${skip[@]}" 2>&1 | grep -E '^test result|^error' | tail -1
  echo "최종: cargo build --release --locked -p photocraft (위 환경 변수 그대로)"
}

case "${1:-}" in
  status) cmd_status ;;
  check) shift; cmd_check "$@" ;;
  verify) cmd_verify ;;
  test) cmd_test ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac
