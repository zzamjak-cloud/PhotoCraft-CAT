//! 포크 업데이터 설정 검증: 업데이트가 포크 저장소 릴리스로만 향하고, 릴리스 워크플로가 만드는
//! 에셋 이름을 craft-update 가 플랫폼별로 올바르게 고르는지 확인한다.

use craft_update::{Arch, Os, PackageKind, Platform, pick_asset};

use crate::UPDATE_APP;

const FORK_REPO: &str = "zzamjak-cloud/PhotoCraft-CAT";

#[test]
fn updates_come_only_from_the_fork_repository() {
    assert_eq!(UPDATE_APP.repo, FORK_REPO);
    assert!(!UPDATE_APP.repo.starts_with("storytold/"), "공식 PhotoCAT 저장소를 가리키면 안 된다");
    assert_eq!(UPDATE_APP.releases_url(), "https://github.com/zzamjak-cloud/PhotoCraft-CAT/releases");
    assert_eq!(UPDATE_APP.id, "photocraft-cat");
    assert_ne!(UPDATE_APP.name, craft_update::suite::PHOTOCRAFT.name, "업데이터 캐시 폴더가 공식 앱과 겹치면 안 된다");
}

/// 다른 저장소(공식 PhotoCraft)의 다운로드 링크는 릴리스 응답에 섞여 있어도 버려진다.
#[test]
fn downloads_outside_the_fork_are_dropped() {
    let json = r#"{
        "tag_name": "v9.0.0", "draft": false, "prerelease": false,
        "html_url": "https://github.com/zzamjak-cloud/PhotoCraft-CAT/releases/tag/v9.0.0",
        "assets": [
            {"name": "photocraft-cat-9.0.0-macos-aarch64.tar.gz", "size": 1,
             "browser_download_url": "https://github.com/storytold/photocraft/releases/download/v9.0.0/photocraft-cat-9.0.0-macos-aarch64.tar.gz"},
            {"name": "photocraft-cat-9.0.0-windows-x64-portable.zip", "size": 1,
             "browser_download_url": "https://github.com/zzamjak-cloud/PhotoCraft-CAT/releases/download/v9.0.0/photocraft-cat-9.0.0-windows-x64-portable.zip"}
        ]
    }"#;
    let release = craft_update::parse_github_release(json, UPDATE_APP.repo).expect("릴리스 파싱");
    let names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["photocraft-cat-9.0.0-windows-x64-portable.zip"]);
}

fn release(names: &[&str]) -> craft_update::Release {
    craft_update::Release {
        tag: "v0.19.1".into(),
        version: craft_update::parse_version("v0.19.1"),
        assets: names.iter().map(|n| craft_update::Asset { name: (*n).into(), size: 1, url: String::new(), sha256: None }).collect(),
        ..Default::default()
    }
}

/// `.github/workflows/release.yml` 이 올리는 에셋 이름과 같아야 한다.
const RELEASE_ASSETS: [&str; 4] = [
    "photocraft-cat-0.19.1-macos-aarch64.dmg",
    "photocraft-cat-0.19.1-macos-aarch64.tar.gz",
    "photocraft-cat-0.19.1-windows-x64-portable.zip",
    "SHA256SUMS.txt",
];

#[test]
fn release_assets_match_each_platform() {
    let r = release(&RELEASE_ASSETS);
    let stems: Vec<&str> = UPDATE_APP.stems().collect();
    let pick = |os, arch, kind| pick_asset(&r, &stems, Platform { os, arch }, &[kind]).map(|(a, _)| a.name.clone());
    // macOS 자체 업데이트는 포크 전용 mac_update 가 이 tarball 을 쓴다.
    assert_eq!(pick(Os::MacOs, Arch::Arm64, PackageKind::TarGz).as_deref(), Some("photocraft-cat-0.19.1-macos-aarch64.tar.gz"));
    assert_eq!(pick(Os::MacOs, Arch::Arm64, PackageKind::Dmg).as_deref(), Some("photocraft-cat-0.19.1-macos-aarch64.dmg"));
    assert_eq!(pick(Os::Windows, Arch::X64, PackageKind::PortableZip).as_deref(), Some("photocraft-cat-0.19.1-windows-x64-portable.zip"));
    // 인텔 Mac 에는 ARM 빌드를 주지 않는다.
    assert_eq!(pick(Os::MacOs, Arch::X64, PackageKind::TarGz), None);
}

#[test]
fn official_photocraft_asset_names_are_ignored() {
    let r = release(&["photocraft-0.19.1-macos-aarch64.tar.gz", "photocraft-0.19.1-windows-x64-portable.zip"]);
    let stems: Vec<&str> = UPDATE_APP.stems().collect();
    for (os, arch, kind) in [(Os::MacOs, Arch::Arm64, PackageKind::TarGz), (Os::Windows, Arch::X64, PackageKind::PortableZip)] {
        assert!(pick_asset(&r, &stems, Platform { os, arch }, &[kind]).is_none());
    }
}

/// macOS 번들 id 가 craft-update 의 `ai.storyteller.<id>` 와 같으면 Developer ID(공식 팀) 서명을
/// 요구하는 DMG 경로로 가서 ad-hoc 서명 빌드는 업데이트에 실패한다. 다르게 유지해야 한다.
#[test]
fn macos_bundle_id_avoids_the_developer_id_dmg_path() {
    let script = include_str!("../../../packaging/macos/package-cat.sh");
    assert!(script.contains("BUNDLE_ID=\"com.zzamjak.photocraft-cat\""), "번들 id 정의가 바뀌었다");
    assert!(!script.contains(&UPDATE_APP.bundle_id()));
}

/// 태그는 사전 릴리스 꼬리표 없는 `vX.Y.Z` 여야 한다. craft-update 는 안정 버전만 제안한다.
#[test]
fn only_plain_semver_tags_are_offered() {
    let parse = |t: &str| craft_update::parse_version(t).expect("버전");
    assert!(craft_update::version::is_stable(&parse("v0.19.1")));
    assert!(!craft_update::version::is_stable(&parse("v0.19.1-cat.1")));
    assert!(craft_update::version::offers_update(&parse("0.19.1"), &parse("0.19.0")));
    assert!(!craft_update::version::offers_update(&parse("0.19.0"), &parse("0.19.0")));
}
