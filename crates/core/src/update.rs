// Проверка обновлений по GitHub Releases (Android — .apk, iOS — .ipa).

use serde::Deserialize;

use crate::api::ApiRequest;

const RELEASES_URL: &str = "https://api.github.com/repos/vodolazny/burmalda57/releases/latest";

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
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

/// Разобрать ответ GitHub. `pkg_ext` — ".apk" или ".ipa".
pub fn parse_update(raw: &str, current: &str, pkg_ext: &str) -> Option<UpdateInfo> {
    let rel: GhRelease = serde_json::from_str(raw).ok()?;
    // В монорепо теги платформенные: android-vX.Y.Z / ios-vX.Y.Z.
    // Поддерживаем и старые vX.Y.Z / X.Y.Z.
    let latest = rel
        .tag_name
        .trim_start_matches("android-")
        .trim_start_matches("ios-")
        .trim_start_matches('v');
    if !is_newer(latest, current) {
        return None;
    }
    let url = rel
        .assets
        .iter()
        .find(|a| a.name.ends_with(pkg_ext))
        .map(|a| a.browser_download_url.clone())
        .unwrap_or(rel.html_url);
    Some(UpdateInfo {
        version: latest.to_string(),
        url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_is_semver_only() {
        assert!(is_newer("1.7.2", "1.7.1"));
        assert!(is_newer("android-1.8.0", "1.7.1") == false); // префикс снимается до вызова
        assert!(!is_newer("nightly", "1.7.1"));
        assert!(!is_newer("1.7.1", "1.7.1"));
    }

    #[test]
    fn picks_asset_by_extension() {
        let raw = r#"{
            "tag_name":"v1.8.0",
            "html_url":"https://github.com/x/y/releases/tag/v1.8.0",
            "assets":[
                {"name":"android-1.8.0.apk","browser_download_url":"https://x/app.apk"},
                {"name":"ios-1.8.0.ipa","browser_download_url":"https://x/app.ipa"}
            ]
        }"#;
        assert_eq!(
            parse_update(raw, "1.7.1", ".apk"),
            Some(UpdateInfo { version: "1.8.0".into(), url: "https://x/app.apk".into() })
        );
        assert_eq!(
            parse_update(raw, "1.7.1", ".ipa").unwrap().url,
            "https://x/app.ipa"
        );
        assert_eq!(parse_update(raw, "1.8.0", ".apk"), None);
    }

    #[test]
    fn falls_back_to_release_page() {
        let raw = r#"{"tag_name":"v2.0.0","html_url":"https://x/page","assets":[]}"#;
        assert_eq!(parse_update(raw, "1.0.0", ".apk").unwrap().url, "https://x/page");
    }
}
