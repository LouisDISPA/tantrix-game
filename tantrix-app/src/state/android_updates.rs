//! Direct APK updates. Android verifies the signing key during installation.

pub const RELEASES_URL: &str = "https://github.com/LouisDISPA/tantrix-game/releases/latest";

#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
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
    let present = release["assets"].as_array().is_some_and(|assets| {
        assets.iter().any(|asset| {
            asset["name"].as_str() == Some(&name)
                && asset["browser_download_url"].as_str() == Some(&url)
        })
    });
    Ok(present.then(|| Update {
        version: version.into(),
        url,
    }))
}
