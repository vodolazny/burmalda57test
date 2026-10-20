use serde::Deserialize;

use crate::api::ApiRequest;

// Список релизов, а не /releases/latest: в монорепо релизы платформенные
// (android-vX.Y.Z / ios-vX.Y.Z), а «latest» у репозитория один. С /latest
// iOS-приложение сравнивало бы свою версию с версией Android-релиза.
const RELEASES_URL: &str =
    "https://api.github.com/repos/vodolazny/burmalda57test/releases?per_page=20";

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

pub fn request() -> ApiRequest {
    ApiRequest::get(RELEASES_URL, 10)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (
        semver::Version::parse(latest),
        semver::Version::parse(current),
    ) {
        (Ok(l), Ok(c)) => l > c,
        // Не-semver тег не считаем обновлением: иначе любой отличающийся тег
        // (в т.ч. откат релиза) показывал бы ложный баннер.
        _ => false,
    }
}

/// Версия из тега, если тег относится к платформе пакета
/// (`pkg_ext` — ".apk" или ".ipa"). Android понимает и старые теги `vX.Y.Z`,
/// выпущенные до перехода на монорепо.
fn platform_version<'a>(tag: &'a str, pkg_ext: &str) -> Option<&'a str> {
    match pkg_ext {
        ".apk" => tag
            .strip_prefix("android-v")
            .or_else(|| tag.strip_prefix('v')),
        ".ipa" => tag.strip_prefix("ios-v"),
        _ => None,
    }
}

/// Разобрать список релизов GitHub. `pkg_ext` — ".apk" или ".ipa".
pub fn parse_update(raw: &str, current: &str, pkg_ext: &str) -> Option<UpdateInfo> {
    let releases: Vec<GhRelease> = serde_json::from_str(raw).ok()?;
    // Самая новая по semver версия среди релизов этой платформы;
    // порядок в ответе GitHub не важен, черновики и пре-релизы пропускаем.
    let (_, latest, rel) = releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = platform_version(&r.tag_name, pkg_ext)?.to_string();
            let parsed = semver::Version::parse(&version).ok()?;
            Some((parsed, version, r))
        })
        .max_by(|a, b| a.0.cmp(&b.0))?;
    if !is_newer(&latest, current) {
        return None;
    }
    let url = rel
        .assets
        .iter()
        .find(|a| a.name.ends_with(pkg_ext))
        .map(|a| a.browser_download_url.clone())
        .unwrap_or(rel.html_url);
    Some(UpdateInfo {
        version: latest,
        url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES: &str = r#"[
        {
            "tag_name": "ios-v1.0.0",
            "html_url": "https://x/ios-1.0.0",
            "assets": [
                {"name": "burmalda57-1.0.0-unsigned.ipa", "browser_download_url": "https://x/app.ipa"}
            ]
        },
        {
            "tag_name": "android-v1.7.2",
            "html_url": "https://x/android-1.7.2",
            "assets": [
                {"name": "burmalda57-1.7.2.apk", "browser_download_url": "https://x/app-1.7.2.apk"}
            ]
        },
        {
            "tag_name": "android-v1.8.0",
            "html_url": "https://x/android-1.8.0",
            "assets": [
                {"name": "burmalda57-1.8.0.apk", "browser_download_url": "https://x/app-1.8.0.apk"}
            ]
        },
        {
            "tag_name": "ios-v9.0.0",
            "html_url": "https://x/ios-draft",
            "draft": true,
            "assets": [
                {"name": "app.ipa", "browser_download_url": "https://x/draft.ipa"}
            ]
        },
        {
            "tag_name": "ios-v9.1.0",
            "html_url": "https://x/ios-prerelease",
            "prerelease": true,
            "assets": [
                {"name": "app.ipa", "browser_download_url": "https://x/prerelease.ipa"}
            ]
        },
        {
            "tag_name": "ios-latest",
            "html_url": "https://x/ios-latest",
            "assets": [
                {"name": "app.ipa", "browser_download_url": "https://x/latest.ipa"}
            ]
        }
    ]"#;

    #[test]
    fn picks_highest_version_for_platform() {
        let apk = parse_update(RELEASES, "1.7.1", ".apk").unwrap();
        assert_eq!(apk.version, "1.8.0");
        assert_eq!(apk.url, "https://x/app-1.8.0.apk");

        let ipa = parse_update(RELEASES, "0.9.0", ".ipa").unwrap();
        assert_eq!(ipa.version, "1.0.0");
        assert_eq!(ipa.url, "https://x/app.ipa");
    }

    #[test]
    fn ignores_drafts_prereleases_and_other_platforms() {
        assert!(parse_update(RELEASES, "1.8.0", ".apk").is_none());
        assert!(parse_update(RELEASES, "1.0.0", ".ipa").is_none());
    }

    #[test]
    fn accepts_legacy_android_tag() {
        let raw = r#"[{
            "tag_name": "v2.0.0",
            "html_url": "https://x/android-2.0.0",
            "assets": [
                {"name": "app-2.0.0.apk", "browser_download_url": "https://x/app-2.0.0.apk"}
            ]
        }]"#;
        assert_eq!(parse_update(raw, "1.7.3", ".apk").unwrap().version, "2.0.0");
    }

    #[test]
    fn falls_back_to_release_page_without_asset() {
        let raw = r#"[{
            "tag_name": "android-v2.0.0",
            "html_url": "https://x/page",
            "assets": []
        }]"#;
        assert_eq!(parse_update(raw, "1.7.3", ".apk").unwrap().url, "https://x/page");
    }

    #[test]
    fn version_comparison_is_semver_only() {
        assert!(is_newer("1.7.3", "1.7.2"));
        assert!(!is_newer("1.7.3", "1.7.3"));
        assert!(!is_newer("nightly", "1.7.2"));
        assert!(!is_newer("1.7.2", "nightly"));
    }
}
