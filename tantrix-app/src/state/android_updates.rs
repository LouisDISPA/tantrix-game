//! Direct APK updates. Android verifies the signing key during installation.

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
        .get("https://api.github.com/repos/LouisDISPA/tantrix-game/releases/latest")
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
    let release: serde_json::Value =
        serde_json::from_str(body).map_err(|error| error.to_string())?;
    if release["draft"] == true || release["prerelease"] == true {
        return Ok(None);
    }
    let Some(version) = release["tag_name"]
        .as_str()
        .and_then(|tag| tag.strip_prefix("tantrix-app-v"))
    else {
        return Ok(None);
    };
    let latest = semver::Version::parse(version).map_err(|error| error.to_string())?;
    let installed = semver::Version::parse(installed).map_err(|error| error.to_string())?;
    if !latest.pre.is_empty() || latest <= installed {
        return Ok(None);
    }
    let name = format!("Tantrix-{version}-android-aarch64.apk");
    let url = format!("https://github.com/LouisDISPA/tantrix-game/releases/download/tantrix-app-v{version}/{name}");
    let asset = release["assets"].as_array().and_then(|assets| {
        assets.iter().find(|asset| {
            asset["name"].as_str() == Some(&name)
                && asset["browser_download_url"].as_str() == Some(&url)
        })
    });
    let Some(size) = asset
        .and_then(|asset| asset["size"].as_u64())
        .filter(|size| (1..=512 * 1024 * 1024).contains(size))
    else {
        return Ok(None);
    };
    Ok(Some(Update {
        version: version.into(),
        url,
        size,
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
