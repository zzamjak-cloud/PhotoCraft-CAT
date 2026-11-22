# CLAUDE.md — PhotoCAT (PhotoCraft 포크)

PhotoCraft(Rust/egui) 비상업 개인 포크. 표시 이름은 **PhotoCAT**, 저장소 이름과 내부 id(`photocraft-cat`, `com.zzamjak.photocraft-cat`, 에셋 파일명, 설정 폴더, `.pcraft`)는 업데이트·호환을 위해 유지한다. ArtCraft 상표 조건에 따라 앱·README 에 ArtCraft 로고와 공식 사이트 링크를 넣지 않는다(출처 문구 "Based on PhotoCraft by the ArtCraft team" 만 허용). CAT 플러그인(Photoshop UXP)을 네이티브 도크 패널 **CAT** (탭: `CAT Tools`, `9-Slice Images`)로 이식한다.
업스트림 규칙은 `AGENTS.md`. 이 문서는 CAT 작업 규칙만 담는다.

- 주석·설명·커밋 메시지는 한국어, 식별자는 영어. UI 라벨은 CAT 영문 라벨 그대로.
- 참고 구현(읽기 전용): 원본 CAT `/Users/woody/dev/AI/uxp/CAT_Plugin`, JP_Plugin(TS 이식판) `/Users/woody/dev/AI/JP_Plugin`.
- 비테스트 코드에서 `unwrap/expect/panic` 금지, `unsafe` 금지, 입력값 인덱싱은 `get()`.

## 모듈 구조 (`crates/ui-egui/src/cat/`)

| 경로 | 역할 |
|---|---|
| `mod.rs` | `CatState`(앱의 `app.cat`): 설정, 히스토리 그룹 키, 임시 문서 목록, 복사한 가이드, 진행률, 모달 큐. 진입점 `tools_panel` / `nine_slice_panel` |
| `store.rs` | 영속 데이터: `<설정 폴더>/cat/settings.json` 등. 데스크톱 `main.rs` 가 `cat::set_data_dir` 로 지정, 미지정(테스트·웹)이면 디스크에 쓰지 않음 |
| `lib/` | PhotoCraft 조작 라이브러리. **엔진 명령 문자열은 여기에만** (`core, doc, layer, export, select, guide, pixels, shape`) |
| `features/` | 기능 로직 (CAT 기능 파일 1:1). UI 없음, `lib` 만 사용. 각 파일의 `run(...)` 이 버튼 진입점 |
| `ui/` | 공용 위젯(`mod.rs`) + 섹션별 파일(`export, import, rename, guide, extra, nine_slice`), 아이콘(`icons/`, `include_bytes!`) |
| `tests.rs` | 인프로세스 테스트 + 헬퍼(`test_app`, `new_doc`, `rect_layer`) |
| `ui/vector.rs`, `features/vector_ops.rs` | **VECTOR** 섹션: Pathfinder 9종·Simplify·Smooth·Offset path·Image Trace 창 |

벡터 엔진: `crates/vc-geom`·`vc-trace`·`vc-pathops` (패키지 `vectorcraft-*`, storytold/vectorcraft 에서 벤더링, `ATTRIBUTION.md`). 셰이프 ⇄ `PathData` 변환은 `lib/shape.rs` (`shape_path_region` = op 를 불리언으로 푼 nonzero 영역, `path_data_to_shape_path`, `map_subpaths`). Image Trace 기본 엔진은 `vectorcraft_trace`(`image_trace::run_vector`), CAT 원본은 Classic(`run`). Outline stroke/Expand path 는 `vectorcraft_pathops::{outline_stroke, offset_path}`.

도크 연결: `dock.rs`(`Group::Cat`), `state.rs`(`Panels.cat`, 기본 true), `panels/dock.rs`(레일·본문), `panels/panel_docking.rs`(`catTools`/`nineSlice`), `menu_catalog.rs`(`window.panel.cat`), `menus/invoke.rs`.

## 라이브러리 우선 원칙

1. 기능 파일에서 `app.run` / `session.execute` / 엔진 명령 문자열을 직접 쓰지 않는다. `cat::lib` 함수를 쓴다.
2. 새 조작이 필요하면 `cat/lib/` 에서 동일·유사 함수를 먼저 찾고, 없으면 대상에 맞는 lib 파일에 추가한다 (레이어→`layer`, 문서→`doc`, 저장/열기→`export`, 가이드→`guide`, 선택→`select`, 픽셀→`pixels`, 셰이프/패스→`shape`).
3. 기존 함수 확장은 옵셔널 파라미터(`Option`/옵션 구조체)로 하위 호환 유지.

## PhotoCraft 내부 API 함정

- **동기 실행**: 데스크톱 앱은 `background_jobs = true` 라 `app.run` 이 긴 명령을 백그라운드 잡으로 넘기고 `{"pending":true}` 만 돌려준다. CAT 은 결과가 바로 필요하므로 항상 `lib::core::exec` (잡 끄고 실행) 또는 `run_safe` 안에서 실행한다.
- **Undo 한 단계**: `lib::core::with_history(app, |app| ...)` 안의 명령에는 공통 `coalesce` 키가 붙어 한 단계로 합쳐진다. 직접 문서 편집은 `lib::core::edit` (엔진 `Session::edit_coalesced`, 포크에서 추가) 로 같은 단계에 합류. `file.* / document.*` 명령은 키를 붙이지 않는다.
- **문서 닫기 안전 규칙**: 문서는 항상 `DocId` 로 특정한다(인덱스 보관 금지). `lib::doc::close_doc_without_saving` 은 CAT 임시 문서(`register_temp_doc`, `create_doc_copy_layers`, `duplicate_doc` 가 자동 등록)이거나 변경 없는 문서만 닫고, 사용자의 미저장 문서는 오류로 거부한다. `file.close` 를 직접 쓰지 않는다.
- **저장**: `lib::export::save_doc_as` 는 `photocraft_io::export` 로 바이트를 만들고 같은 폴더 임시 파일 → rename. 실패해도 원본 파일 보존. 기본은 사본 저장(문서 경로 불변), `adopt_path` 면 Save As. 열린 문서의 원본 파일은 내보내기로 덮어쓰지 않는다(`ensure_not_open_document_path`).
- **열기**: `lib::export::open_file` 은 동기 디코드 후 `session.add_document(doc, Some(path))`.
- **winit 은 포크 사본**: `vendor/winit` (storytold/winit `a517b384` 복사, `Cargo.toml` `[patch]` 가 path 로 지정). macOS 2벌식은 조합 표시 없이 `insertText` 로 직전 글자를 교체하며 입력하는데, 원본은 `한`+ㄱ 의 "ㄱ" 을 앱에 보내지 않아 다음 교체가 "한" 을 지웠다(`view.rs` `key_text_undelivered`, 첫 키 재시도 끔). 원본 병합으로 winit rev 가 바뀌면 새 rev 를 다시 복사하고 이 패치를 옮긴다. 검증: `cd vendor/winit && cargo test --test ime_patches`.
- **가이드/잠금/스마트 오브젝트 원본**: 문서 모델을 직접 읽는다 (`doc.guides`, `layer.locks`, `SmartSource::{Embedded, Linked}`).
- **파일 다이얼로그**: 비동기(다음 프레임 응답). `lib::core::pick_files` / `pick_folder` 의 콜백에서 이어서 실행. 그래서 기능 `run` 은 경로를 인자로 받는다.
- **모달**: `lib::core::{alert, confirm, prompt, prompt_size}` → `app.cat.modals` 큐, `ui::show_modals` 가 그린다. 실패 알림은 `run_safe(app, label, f)`.

## 테스트

```bash
cargo test -p photocraft-ui-egui --lib cat::          # CAT 테스트
cargo test -p photocraft-ui-egui --lib dock           # 도크 회귀
cargo build --release -p photocraft                   # 최종 빌드 (번역 포함: CRAFT_I18N_DIR=$PWD/craft-i18n)
CAT_SNAPSHOT_DIR=/tmp/x cargo test -p photocraft-ui-egui --lib cat::tests::the_cat  # 패널 렌더 PNG
```

- 테스트는 `cat::tests::test_app()` (임시 설정 폴더) → `new_doc` → 기능 실행 → 레이어/가이드/파일/`history.past_len()` 검증.
- 테스트 폰트에 한글이 없다: 한국어 모달을 그리는 하네스는 `ctx.set_missing_glyph_policy(Tofu)`.
- 업스트림 도크 레이아웃 불변식 테스트(dock/panel_docking)는 `panels.cat = false` 로 CAT 그룹을 제외하고 검사한다.
- GUI PhotoCraft 프로세스를 띄우거나 종료하지 않는다 (사용자가 실행 중일 수 있음).

## 원본 병합 (storytold/photocraft)

원본 **릴리스 태그** 단위로 가져온다(main 은 미릴리스 대규모 구조 변경이 섞임). 도구: `scripts/upstream-sync.sh`.

1. `status` → 새 태그·커밋 수·main 의 대규모 커밋 확인. `check <태그>` → 위험 보고를 보고 사용자와 범위를 정한다.
2. 브랜치 `merge/upstream-<태그>` 에서 `git merge --no-ff --no-commit refs/upstream-tags/<태그>`.
3. 충돌 원칙: `Cargo.toml` 버전 0.19.0 유지 · `Cargo.lock` 은 원본 것을 받고 cargo 가 다시 맞춘 작업 트리 lock 을 stage · `release.yml`/`ci.yml` 은 포크 것을 두고 원본의 새 필수 단계(외부 저장소 체크아웃, 환경 변수, `craft-fonts-ref` 변경)만 옮긴다 · 포크가 지운 워크플로는 지운 채로, 원본이 새로 넣은 워크플로는 필요 없으면 지운다 · 원본이 지운/옮긴 파일에 포크 수정이 있으면(`check` 1번) 그 내용을 새 위치로 옮긴다.
4. `verify` (마커·버전·미추적·lock·`scripts/upstream-sync-anchors.tsv` 포크 기능) → `test` (craft-i18n·craft-fonts 를 rev 대로 받아 회귀 테스트) → 릴리스 빌드 → 별도 리뷰 에이전트 → 커밋.
5. 포크 기능을 새로 넣으면 `upstream-sync-anchors.tsv` 에 한 줄 추가한다.

- **태그 분리**: 원본 태그는 `refs/upstream-tags/*` 로만 받는다(`remote.upstream.tagOpt --no-tags`). 로컬 `v*` 태그로 섞이면 푸시 시 포크 Release 가 그 번호로 자동 업데이트를 배포한다. `release.yml` 은 원본 태그 번호면 실패한다.
- **번역**: 카탈로그는 외부 craft-i18n(`craft-i18n.rev`)에서 빌드 때 넣는다(`CRAFT_I18N_DIR`, 없으면 경고만 내고 영어 전용). 포크 전용 번역은 `crates/ui-egui/i18n-cat/<locale>.tsv`, `build.rs` `cat_catalog` 가 그 행과 "PhotoCraft→PhotoCAT" 치환 행을 원본 카탈로그 앞에 붙인다. 화면 문구를 추가하면 `i18n-cat/ko.tsv` 에 번역을 넣는다.
- i18n 라이브 테스트는 `CRAFT_FONTS_DIR` 없이는 글리프 없음으로 실패한다(`upstream-sync.sh test` 가 설정).

## 릴리스 / 자동 업데이트

- 업데이터(`crates/ui-egui/src/lib.rs` `UPDATE_APP`)는 `zzamjak-cloud/PhotoCraft-CAT` 릴리스만 본다. 앱 id `photocraft-cat`, 이름 `PhotoCAT`. 검증: `cargo test -p photocraft-ui-egui --lib update_app_tests`.
- 버전은 태그가 결정한다(`PHOTOCRAFT_OVERRIDE_VERSION`). 태그는 반드시 `vX.Y.Z` (꼬리표 금지: craft-update 가 사전 릴리스는 제안하지 않음), 이전 태그보다 커야 한다. Cargo.toml 버전은 0.19.0 그대로 둔다.
- 릴리스: `git tag v0.19.1 && git push origin v0.19.1` → Actions `Release` 가 macOS(arm64)·Windows(x64) 빌드 후 릴리스에 에셋 첨부. 수동: `gh workflow run release.yml -f version=0.19.1`.
- 로컬 패키징: `packaging/macos/package-cat.sh` (ad-hoc 서명 .app/.dmg/.tar.gz → `dist/release/`), Windows 는 `packaging/windows/package-cat.ps1 -Version X.Y.Z` (Inno Setup 6 필요).
- 서명 없음: macOS 첫 실행은 우클릭 → 열기(또는 시스템 설정 → 개인정보 보호 및 보안 → "그래도 열기", 안 되면 `xattr -dr com.apple.quarantine /Applications/PhotoCAT.app`). Windows 는 설치 프로그램 실행 시 SmartScreen "추가 정보 → 실행".
- 자동 설치: macOS 는 포크 전용 `crates/ui-egui/src/mac_update.rs` (craft-update 새 버전은 `ai.storyteller.*` 가 아닌 번들을 자체 업데이트하지 않는다): 릴리스 job 이 tarball 을 같은 키로 서명해 `.tar.gz.sig` 를 올리고, 앱은 `PUBLIC_KEY` 확인 후 도우미 sh(`mac_update_helper.sh`)가 앱 종료 뒤 `Contents/MacOS/PhotoCraft` 를 같은 폴더 rename 으로 교체·ad-hoc 재서명·재실행. 창·실패 기록은 `win_update` 와 공유. Windows 는 craft-update 가 Authenticode 서명을 요구하므로 포크 전용 `crates/ui-egui/src/win_update.rs` 를 쓴다: 릴리스 job 이 설치 프로그램(Inno Setup, `packaging/windows/photocat.iss`, 사용자별 설치)을 Ed25519 키로 서명해 `<setup>.exe.sig` 를 올리고, 앱은 내장 `PUBLIC_KEY` 로 확인 후 도우미 PowerShell(`win_update_helper.ps1`, ASCII 전용)이 앱 종료 뒤 조용히 설치하고 재실행. 실패 사유는 다음 실행 때 상태 표시줄에 뜬다.
- 포터블 전환: v0.19.10 이하 앱은 포터블 zip 만 받을 수 있어 릴리스에 zip·서명을 계속 올린다. 새 버전의 포터블 사본은 시작할 때 설치형 전환을 제안하고, 도우미가 `PhotoCraftData` 를 `%APPDATA%\Photocraft`(복구 파일은 `%LOCALAPPDATA%\Photocraft\Recovery`)로 복사한 뒤 설치한다. 검증: `.github/workflows/windows-installer.yml` (`packaging/windows/test-installer.ps1`).
- 서명 키: 개인키 GitHub Secret `UPDATE_SIGNING_KEY`(Ed25519 PEM, 로컬 백업 `~/.config/photocraft-cat/update-signing-key.pem`). 시크릿이 없으면 Release 의 서명 단계가 실패한다. 키를 바꾸면 `PUBLIC_KEY` 도 바꾸고, 이전 버전 사용자는 수동 설치가 한 번 필요하다.
