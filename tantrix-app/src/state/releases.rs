//! Stable GitHub app releases and package metadata shared by downloads and updates.

#![cfg_attr(not(all(target_arch = "wasm32", feature = "web")), allow(dead_code))]

pub const RELEASE_API: &str =
    "https://api.github.com/repos/LouisDISPA/tantrix-game/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/LouisDISPA/tantrix-game/releases/latest";
const ASSET_URL_PREFIX: &str = "https://github.com/LouisDISPA/tantrix-game/releases/download/";

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
    Ios,
    Android,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Architecture {
    Arm64,
    X64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Dmg,
    AppImage,
    Deb,
    Installer,
    Portable,
    Apk,
}

#[derive(Clone, PartialEq)]
pub struct DownloadAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub platform: Platform,
    pub architecture: Architecture,
    pub package: Package,
}

impl DownloadAsset {
    pub fn parse(name: String, url: String, size: u64, version: &str) -> Option<Self> {
        if url != format!("{ASSET_URL_PREFIX}tantrix-app-v{version}/{name}") {
            return None;
        }
        let suffix = name.strip_prefix(&format!("Tantrix-{version}-"))?;
        let (system, suffix) = suffix.split_once('-')?;
        let platform = match system {
            "macos" => Platform::Mac,
            "windows" => Platform::Windows,
            "linux" => Platform::Linux,
            "android" => Platform::Android,
            _ => return None,
        };
        let (architecture, suffix) = if let Some(suffix) = suffix.strip_prefix("aarch64") {
            (Architecture::Arm64, suffix)
        } else {
            (Architecture::X64, suffix.strip_prefix("x86_64")?)
        };
        let package = match (platform, suffix) {
            (Platform::Mac, ".dmg") => Package::Dmg,
            (Platform::Android, ".apk") => Package::Apk,
            (Platform::Linux, ".AppImage") => Package::AppImage,
            (Platform::Linux, ".deb") => Package::Deb,
            (Platform::Windows, "-setup.exe") => Package::Installer,
            (Platform::Windows, "-portable.exe" | "-portable.zip") => Package::Portable,
            _ => return None,
        };
        Some(Self {
            name,
            url,
            size,
            platform,
            architecture,
            package,
        })
    }

    pub fn label(&self) -> String {
        let system = match self.platform {
            Platform::Mac => "macOS",
            Platform::Windows => "Windows",
            Platform::Android => "Android",
            _ => "Linux",
        };
        let chip = match (self.platform, self.architecture) {
            (Platform::Mac, Architecture::Arm64) => "Apple Silicon",
            (Platform::Mac, Architecture::X64) => "Intel",
            (_, Architecture::Arm64) => "ARM64",
            (_, Architecture::X64) => "x64",
        };
        let package = match self.package {
            Package::Dmg => "DMG",
            Package::AppImage => "AppImage",
            Package::Deb => "Debian / Ubuntu",
            Package::Installer => "Installer",
            Package::Portable => "Portable",
            Package::Apk => "APK",
        };
        format!("{system} · {chip} · {package}")
    }

    pub fn size_mb(&self) -> String {
        format!("{:.1}", self.size as f64 / 1024.0 / 1024.0)
    }
}

#[derive(Clone, PartialEq)]
pub struct Release {
    pub version: semver::Version,
    pub assets: Vec<DownloadAsset>,
}

impl Release {
    /// Ignore non-app and unstable releases. Malformed metadata is an error;
    /// unsupported or malformed individual assets are skipped.
    pub fn parse(body: &str) -> Result<Option<Self>, String> {
        let release: serde_json::Value =
            serde_json::from_str(body).map_err(|error| error.to_string())?;
        if release["draft"].as_bool() != Some(false)
            || release["prerelease"].as_bool() != Some(false)
        {
            return Ok(None);
        }
        let Some(version) = release["tag_name"]
            .as_str()
            .and_then(|tag| tag.strip_prefix("tantrix-app-v"))
        else {
            return Ok(None);
        };
        let parsed_version = semver::Version::parse(version).map_err(|error| error.to_string())?;
        if !parsed_version.pre.is_empty() {
            return Ok(None);
        }
        let raw_assets = release["assets"]
            .as_array()
            .ok_or_else(|| "Missing release assets.".to_string())?;
        let assets = raw_assets
            .iter()
            .filter_map(|asset| {
                DownloadAsset::parse(
                    asset["name"].as_str()?.to_owned(),
                    asset["browser_download_url"].as_str()?.to_owned(),
                    asset["size"].as_u64()?,
                    version,
                )
            })
            .collect();
        Ok(Some(Self {
            version: parsed_version,
            assets,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_stable_app_versions_are_offered() {
        for (tag, draft, prerelease) in [
            ("tantrix-app-v1.2.3", true, false),
            ("tantrix-app-v1.2.3", false, true),
            ("tantrix-app-v1.2.3-rc.1", false, false),
            ("other-v1.2.3", false, false),
        ] {
            let body = json!({
                "tag_name": tag, "draft": draft, "prerelease": prerelease, "assets": []
            });
            assert!(Release::parse(&body.to_string()).unwrap().is_none());
        }
        for version in ["", "1", "1.2", "1..2", "01.2.3", "1.2.3.4"] {
            let body = json!({
                "tag_name": format!("tantrix-app-v{version}"),
                "draft": false, "prerelease": false, "assets": []
            });
            assert!(Release::parse(&body.to_string()).is_err(), "{version}");
        }
        assert!(Release::parse("not JSON").is_err());
        assert!(
            Release::parse(r#"{"tag_name":"tantrix-app-v1.2.3","assets":[]}"#)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stable_releases_keep_only_matching_packages_and_integer_sizes() {
        let mut assets = Vec::new();
        for suffix in [
            "macos-aarch64.dmg",
            "windows-x86_64-setup.exe",
            "linux-x86_64.AppImage",
            "android-aarch64.apk",
        ] {
            let name = format!("Tantrix-1.2.3-{suffix}");
            assets.push(json!({
                "browser_download_url": format!("{ASSET_URL_PREFIX}tantrix-app-v1.2.3/{name}"),
                "name": name, "size": 1024
            }));
        }
        let valid = assets[0].clone();
        for size in [json!(-1), json!(1.5), json!("1024"), json!(null)] {
            let mut invalid = valid.clone();
            invalid["size"] = size;
            assets.push(invalid);
        }
        for url in [
            "https://example.com/Tantrix-1.2.3-macos-aarch64.dmg",
            &format!("{ASSET_URL_PREFIX}tantrix-app-v1.2.3/different.dmg"),
            &format!("{ASSET_URL_PREFIX}tantrix-app-v1.2.4/Tantrix-1.2.3-macos-aarch64.dmg"),
        ] {
            let mut invalid = valid.clone();
            invalid["browser_download_url"] = json!(url);
            assets.push(invalid);
        }
        assets.push(json!({"name": "SHA256SUMS", "size": 100}));
        let body = json!({
            "tag_name": "tantrix-app-v1.2.3", "draft": false,
            "prerelease": false, "assets": assets
        });
        let release = Release::parse(&body.to_string()).unwrap().unwrap();
        assert_eq!(release.version, semver::Version::new(1, 2, 3));
        assert_eq!(release.assets.len(), 4);
        assert!(release.assets.iter().all(|asset| asset.size == 1024));
        let apk = &release.assets[3];
        assert_eq!(apk.platform, Platform::Android);
        assert_eq!(apk.architecture, Architecture::Arm64);
        assert!(matches!(apk.package, Package::Apk));
    }
}
