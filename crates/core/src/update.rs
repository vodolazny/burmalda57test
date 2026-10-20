 
use serde::Deserialize;
 
use crate::api::ApiRequest;
 
// Список релизов, а не /releases/latest: в монорепо релизы платформенные
// (android-vX.Y.Z / ios-vX.Y.Z), а «latest» у репозитория один. С /latest
// iOS-приложение сравнивало бы свою версию с версией Android-релиза.
const RELEASES_URL: &str =
    "https://api.github.com/repos/vodolazny/burmalda57/releases?per_page=20";
 
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
 
