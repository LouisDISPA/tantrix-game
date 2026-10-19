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
        if !url.starts_with(&format!("{ASSET_URL_PREFIX}tantrix-app-v{version}/")) {
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
    pub version: String,
    pub assets: Vec<DownloadAsset>,
}

impl Release {
    pub fn preferred(choices: &[DownloadAsset], device: Device) -> Option<String> {
        let platform = match device.platform {
            Platform::Mac | Platform::Windows | Platform::Linux | Platform::Android => {
                device.platform
            }
            _ => Platform::Windows,
        };
        let architecture = device.architecture.unwrap_or(Architecture::X64);
        choices
            .iter()
            .filter(|asset| asset.platform == platform)
            .min_by_key(|asset| {
                (
                    asset.architecture != architecture,
                    matches!(asset.package, Package::Portable | Package::Deb),
                )
            })
            .map(|asset| asset.name.clone())
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
    fn defaults_use_detected_platform_and_architecture() {
        let choices = vec![
            asset("macos-aarch64.dmg"),
            asset("macos-x86_64.dmg"),
            asset("windows-aarch64-setup.exe"),
            asset("windows-x86_64-portable.exe"),
            asset("windows-x86_64-setup.exe"),
            asset("linux-x86_64.deb"),
            asset("linux-x86_64.AppImage"),
            asset("android-aarch64.apk"),
        ];
        for (platform, architecture, expected) in [
            (Platform::Mac, Some(Architecture::Arm64), 0),
            (Platform::Mac, Some(Architecture::X64), 1),
            (Platform::Mac, None, 1),
            (Platform::Windows, Some(Architecture::Arm64), 2),
            (Platform::Windows, None, 4),
            (Platform::Linux, Some(Architecture::X64), 6),
            (Platform::Unknown, None, 4),
            (Platform::Ios, None, 4),
            (Platform::Android, Some(Architecture::Arm64), 7),
            (Platform::Android, None, 7),
        ] {
            assert_eq!(
                Release::preferred(
                    &choices,
                    Device {
                        platform,
                        architecture,
                        safari: false
                    }
                ),
                Some(choices[expected].name.clone())
            );
        }
        assert_eq!(Release::preferred(&[], Device::default()), None);
    }
}
