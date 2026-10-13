//! Download metadata and platform decisions, independent of the browser UI.

// SSR renders the same models but only the browser constructs detected
// platforms, parsed release assets, and installed states.
#![cfg_attr(not(all(target_arch = "wasm32", feature = "web")), allow(dead_code))]

use dioxus::prelude::*;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub mod browser;

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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Device {
    pub platform: Platform,
    pub architecture: Option<Architecture>,
    pub safari: bool,
}

impl Device {
    pub fn detect(user_agent: &str, touch_points: i32) -> Self {
        let contains = |names: &[&str]| names.iter().any(|name| user_agent.contains(name));
        let platform = if contains(&["iPhone", "iPad", "iPod"])
            || (user_agent.contains("Mac") && touch_points > 1)
        {
            Platform::Ios
        } else if user_agent.contains("Android") {
            Platform::Android
        } else if user_agent.contains("Windows") {
            Platform::Windows
        } else if user_agent.contains("Mac") {
            Platform::Mac
        } else if user_agent.contains("Linux") {
            Platform::Linux
        } else {
            Platform::Unknown
        };
        Self {
            platform,
            architecture: None,
            safari: user_agent.contains("Safari")
                && !contains(&["Chrome", "Chromium", "CriOS", "Edg", "FxiOS"]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Dmg,
    AppImage,
    Deb,
    Installer,
    Portable,
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
        if !url.starts_with(&format!("{ASSET_URL_PREFIX}tantrix-app-v{version}/")) {
            return None;
        }
        let suffix = name.strip_prefix(&format!("Tantrix-{version}-"))?;
        let (system, suffix) = suffix.split_once('-')?;
        let platform = match system {
            "macos" => Platform::Mac,
            "windows" => Platform::Windows,
            "linux" => Platform::Linux,
            _ => return None,
        };
        let (architecture, suffix) = if let Some(suffix) = suffix.strip_prefix("aarch64") {
            (Architecture::Arm64, suffix)
        } else {
            (Architecture::X64, suffix.strip_prefix("x86_64")?)
        };
        let package = match (platform, suffix) {
            (Platform::Mac, ".dmg") => Package::Dmg,
            (Platform::Linux, ".AppImage") => Package::AppImage,
            (Platform::Linux, ".deb") => Package::Deb,
            (Platform::Windows, "-setup.exe") => Package::Installer,
            (Platform::Windows, "-portable.exe") => Package::Portable,
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
        };
        format!("{system} · {chip} · {package}")
    }

    pub fn size_mb(&self) -> String {
        format!("{:.1}", self.size as f64 / 1024.0 / 1024.0)
    }
}

#[derive(Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub assets: Vec<DownloadAsset>,
}

impl Release {
    pub fn choices(&self, device: Device) -> Vec<DownloadAsset> {
        self.assets
            .iter()
            .filter(|a| a.platform == device.platform)
            .cloned()
            .collect()
    }

    pub fn preferred(choices: &[DownloadAsset], device: Device) -> Option<String> {
        if let Some(asset) = choices.iter().find(|a| {
            a.platform == device.platform
                && Some(a.architecture) == device.architecture
                && !matches!(a.package, Package::Portable | Package::Deb)
        }) {
            return Some(asset.name.clone());
        }
        let first = choices.first()?;
        // Unknown or unavailable architecture must never silently pick a Mac
        // build or an incompatible package for a detected ARM device.
        if choices.iter().any(|a| a.architecture != first.architecture)
            || (first.platform == device.platform
                && device
                    .architecture
                    .is_some_and(|arch| arch != first.architecture))
        {
            None
        } else {
            Some(first.name.clone())
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum InstallStatus {
    #[default]
    Unavailable,
    Available,
    Installed,
}

#[derive(Clone, Copy)]
pub struct DownloadContext {
    pub device: Signal<Device>,
    pub install_status: Signal<InstallStatus>,
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    pub prompt: Signal<Option<browser::InstallPrompt>>,
}

impl DownloadContext {
    pub fn install(self) {
        #[cfg(all(target_arch = "wasm32", feature = "web"))]
        browser::install(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(suffix: &str) -> DownloadAsset {
        let name = format!("Tantrix-0.1.0-{suffix}");
        let url = format!("{ASSET_URL_PREFIX}tantrix-app-v0.1.0/{name}");
        DownloadAsset::parse(name, url, 3 * 1024 * 1024, "0.1.0").unwrap()
    }

    #[test]
    fn ipad_desktop_user_agent_still_prefers_the_web_app() {
        let ipad = Device::detect("Macintosh AppleWebKit Safari", 5);
        assert_eq!(ipad.platform, Platform::Ios);
        assert_eq!(
            Device::detect("Macintosh AppleWebKit Safari", 0).platform,
            Platform::Mac
        );
        assert!(!Device::detect("Android Linux Chrome Safari", 5).safari);
    }

    #[test]
    fn downloads_reject_checksums_wrong_versions_and_untrusted_links() {
        let dmg = asset("macos-aarch64.dmg");
        assert_eq!(dmg.label(), "macOS · Apple Silicon · DMG");
        assert!(DownloadAsset::parse(
            format!("{}.sha256", dmg.name),
            dmg.url.clone(),
            100,
            "0.1.0"
        )
        .is_none());
        assert!(DownloadAsset::parse(dmg.name.clone(), dmg.url.clone(), 100, "0.2.0").is_none());
        assert!(
            DownloadAsset::parse(dmg.name, "https://example.com/app.dmg".into(), 100, "0.1.0")
                .is_none()
        );
    }

    #[test]
    fn detection_never_guesses_unknown_or_unavailable_architecture() {
        let choices = vec![asset("macos-aarch64.dmg"), asset("macos-x86_64.dmg")];
        let mac = Device {
            platform: Platform::Mac,
            ..Device::default()
        };
        assert_eq!(Release::preferred(&choices, mac), None);
        assert_eq!(
            Release::preferred(
                &choices,
                Device {
                    architecture: Some(Architecture::X64),
                    ..mac
                }
            ),
            Some(choices[1].name.clone())
        );
        let windows_arm = Device {
            platform: Platform::Windows,
            architecture: Some(Architecture::Arm64),
            safari: false,
        };
        assert_eq!(
            Release::preferred(&[asset("windows-x86_64-setup.exe")], windows_arm),
            None
        );
    }

    #[test]
    fn linux_prefers_appimage_and_only_offers_matching_platforms() {
        let release = Release {
            version: "0.1.0".into(),
            assets: vec![
                asset("macos-aarch64.dmg"),
                asset("linux-x86_64.deb"),
                asset("linux-x86_64.AppImage"),
            ],
        };
        let linux = Device {
            platform: Platform::Linux,
            architecture: Some(Architecture::X64),
            safari: false,
        };
        let choices = release.choices(linux);
        assert_eq!(choices.len(), 2);
        assert_eq!(
            Release::preferred(&choices, linux),
            Some(choices[1].name.clone())
        );
        assert_eq!(
            release
                .choices(Device {
                    platform: Platform::Ios,
                    ..Device::default()
                })
                .len(),
            0
        );
    }
}
