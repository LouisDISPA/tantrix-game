//! Direct APK updates. Android verifies the signing key during installation.

use super::releases::{Architecture, Package, Platform, Release, RELEASE_API};

#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
    pub size: u64,
}

pub async fn check() -> Result<Option<Update>, String> {
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Tantrix")
        .build()
        .map_err(|error| error.to_string())?
        .get(RELEASE_API)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .await
        .map_err(|error| error.to_string())?;
    parse_release(&response, env!("CARGO_PKG_VERSION"))
}

fn parse_release(body: &str, installed: &str) -> Result<Option<Update>, String> {
    let Some(release) = Release::parse(body)? else {
        return Ok(None);
    };
    let installed = semver::Version::parse(installed).map_err(|error| error.to_string())?;
    if release.version <= installed {
        return Ok(None);
    }
    let asset = release.assets.into_iter().find(|asset| {
        asset.platform == Platform::Android
            && asset.architecture == Architecture::Arm64
            && asset.package == Package::Apk
            && (1..=512 * 1024 * 1024).contains(&asset.size)
    });
    Ok(asset.map(|asset| Update {
        version: release.version.to_string(),
        url: asset.url,
        size: asset.size,
    }))
}

/// Stream into a private partial file; only expose a completed APK to the installer.
pub async fn download(
    update: &Update,
    mut progress: impl FnMut(u8),
) -> Result<std::path::PathBuf, String> {
    use tokio::io::AsyncWriteExt;
    let directory = super::native_settings::settings_dir()?.join("updates");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let partial = directory.join("update.part");
    let completed = directory.join("update.apk");
    let result = async {
        let mut response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|error| error.to_string())?
            .get(&update.url)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|error| error.to_string())?;
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            received += chunk.len() as u64;
            if received > update.size {
                return Err("The APK download has an unexpected size.".into());
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| error.to_string())?;
            progress((received * 100 / update.size) as u8);
        }
        if received != update.size {
            return Err("The APK download is incomplete. Try again.".into());
        }
        file.flush().await.map_err(|error| error.to_string())?;
        drop(file);
        tokio::fs::rename(&partial, &completed)
            .await
            .map_err(|error| error.to_string())?;
        Ok(completed)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
}

pub async fn install(path: std::path::PathBuf) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    return crate::platform::android::install_update(path).await;
    #[cfg(not(target_os = "android"))]
    {
        let _ = path;
        Err("APK installation requires Android.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn updates_require_a_newer_version_and_a_bounded_arm64_apk() {
        let name = "Tantrix-1.2.3-android-aarch64.apk";
        let url = format!("https://github.com/LouisDISPA/tantrix-game/releases/download/tantrix-app-v1.2.3/{name}");
        let mut release = json!({
            "tag_name": "tantrix-app-v1.2.3", "draft": false, "prerelease": false,
            "assets": [{"name": name, "browser_download_url": url, "size": 1024}]
        });
        assert_eq!(
            parse_release(&release.to_string(), "1.2.2").unwrap(),
            Some(Update {
                version: "1.2.3".into(),
                url,
                size: 1024,
            })
        );
        for installed in ["1.2.3", "1.3.0", "2.0.0"] {
            assert_eq!(
                parse_release(&release.to_string(), installed).unwrap(),
                None
            );
        }
        for size in [0, 512 * 1024 * 1024 + 1] {
            release["assets"][0]["size"] = json!(size);
            assert_eq!(parse_release(&release.to_string(), "1.2.2").unwrap(), None);
        }
        release["assets"][0]["size"] = json!(1024);
        release["assets"][0]["browser_download_url"] = json!("https://example.com/update.apk");
        assert_eq!(parse_release(&release.to_string(), "1.2.2").unwrap(), None);
    }
}
