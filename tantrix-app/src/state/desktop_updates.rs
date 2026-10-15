//! Velopack owns package discovery, verification and replacement. One worker
//! serializes operations; UI state crosses back through an async channel.

use std::sync::mpsc::{self, Receiver, Sender};

use futures_channel::mpsc::UnboundedSender;
use velopack::{sources::GithubSource, UpdateCheck, UpdateManager, VelopackAsset};

pub use super::online_compatibility::DOWNLOAD_URL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    Checking,
    Current,
    Downloading { version: String, percent: i16 },
    Ready { version: String },
    Installing,
    Failed { version: Option<String> },
    Manual,
}

impl UpdateStatus {
    pub fn is_outdated(&self) -> bool {
        matches!(
            self,
            Self::Downloading { .. }
                | Self::Ready { .. }
                | Self::Installing
                | Self::Failed { version: Some(_) }
        )
    }
}

pub fn run(commands: Receiver<()>, events: UnboundedSender<UpdateStatus>) {
    let source = GithubSource::new(
        "https://github.com/LouisDISPA/tantrix-game",
        None,
        env!("CARGO_PKG_VERSION").contains('-'),
    );
    let manager = match UpdateManager::new(source, None, None) {
        Ok(manager) => manager,
        // Development builds and .deb/legacy installs have no Velopack layout.
        Err(error) => {
            log::debug!("Automatic updates unavailable for this installation: {error}");
            let _ = events.unbounded_send(UpdateStatus::Manual);
            return;
        }
    };
    // Exactly one release check per app launch. Menu navigation and online
    // compatibility failures reuse the result; there is no check command.
    let pending = match check_and_download(&manager, &events) {
        Ok(asset) => asset,
        Err(error) => {
            log::warn!("Desktop update failed: {error}");
            None
        }
    };
    while commands.recv().is_ok() {
        if let Some(asset) = &pending {
            let _ = events.unbounded_send(UpdateStatus::Installing);
            if let Err(error) = manager.apply_updates_and_restart(asset) {
                log::warn!("Could not install desktop update: {error}");
                let _ = events.unbounded_send(UpdateStatus::Failed {
                    version: Some(asset.Version.clone()),
                });
            }
        }
    }
}

fn check_and_download(
    manager: &UpdateManager,
    events: &UnboundedSender<UpdateStatus>,
) -> Result<Option<VelopackAsset>, velopack::Error> {
    let _ = events.unbounded_send(UpdateStatus::Checking);
    if let Some(asset) = manager.get_update_pending_restart() {
        let _ = events.unbounded_send(UpdateStatus::Ready {
            version: asset.Version.clone(),
        });
        return Ok(Some(asset));
    }
    let update = match manager.check_for_updates().inspect_err(|_| {
        let _ = events.unbounded_send(UpdateStatus::Failed { version: None });
    })? {
        UpdateCheck::UpdateAvailable(update) => update,
        _ => {
            let _ = events.unbounded_send(UpdateStatus::Current);
            return Ok(None);
        }
    };
    let version = update.TargetFullRelease.Version.clone();
    let _ = events.unbounded_send(UpdateStatus::Downloading {
        version: version.clone(),
        percent: 0,
    });
    // Velopack's synchronous API reports progress through std::mpsc. Forward
    // it while the download runs, without polling or touching Dioxus signals.
    let result = std::thread::scope(|scope| {
        let (tx, rx): (Sender<i16>, Receiver<i16>) = mpsc::channel();
        let download = scope.spawn(|| manager.download_updates(&update, Some(tx)));
        for percent in rx {
            let _ = events.unbounded_send(UpdateStatus::Downloading {
                version: version.clone(),
                percent: percent.clamp(0, 100),
            });
        }
        download
            .join()
            .unwrap_or_else(|_| Err(velopack::Error::Other("Update worker stopped".into())))
    });
    if result.is_err() {
        let _ = events.unbounded_send(UpdateStatus::Failed {
            version: Some(version.clone()),
        });
    }
    result?;
    let _ = events.unbounded_send(UpdateStatus::Ready { version });
    Ok(Some(update.TargetFullRelease))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use velopack::{locator::VelopackLocatorConfig, sources::FileSource, VelopackAssetFeed};

    fn installation(root: &std::path::Path, version: &str) -> UpdateManager {
        let manifest = root.join("sq.version");
        fs::write(&manifest, format!(r#"<package><metadata><id>Tantrix</id><version>{version}</version><channel>test</channel><mainExe>tantrix</mainExe></metadata></package>"#)).unwrap();
        let helper = root.join("Update");
        fs::write(&helper, "").unwrap();
        fs::create_dir(root.join("packages")).unwrap();
        UpdateManager::new(
            FileSource::new(root),
            None,
            Some(VelopackLocatorConfig {
                RootAppDir: root.into(),
                UpdateExePath: helper,
                PackagesDir: root.join("packages"),
                ManifestPath: manifest,
                CurrentBinaryDir: root.into(),
                IsPortable: true,
            }),
        )
        .unwrap()
    }

    #[test]
    fn matching_release_stays_current_without_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let manager = installation(dir.path(), "1.0.0");
        let asset = VelopackAsset {
            Version: "1.0.0".into(),
            Type: "Full".into(),
            ..Default::default()
        };
        fs::write(
            dir.path().join("releases.test.json"),
            serde_json::to_vec(&VelopackAssetFeed {
                Assets: vec![asset],
            })
            .unwrap(),
        )
        .unwrap();
        let (events, mut rx) = futures_channel::mpsc::unbounded();
        assert!(check_and_download(&manager, &events).unwrap().is_none());
        drop(events);
        let statuses: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(statuses, [UpdateStatus::Checking, UpdateStatus::Current]);
    }

    #[test]
    fn corrupt_download_never_becomes_ready_to_install() {
        let dir = tempfile::tempdir().unwrap();
        let manager = installation(dir.path(), "1.0.0");
        let asset = VelopackAsset {
            PackageId: "Tantrix".into(),
            Version: "2.0.0".into(),
            Type: "Full".into(),
            FileName: "Tantrix-2.0.0-full.nupkg".into(),
            Size: 5,
            SHA256: "0".repeat(64),
            ..Default::default()
        };
        fs::write(dir.path().join(&asset.FileName), "wrong").unwrap();
        fs::write(
            dir.path().join("releases.test.json"),
            serde_json::to_vec(&VelopackAssetFeed {
                Assets: vec![asset.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        let (events, mut rx) = futures_channel::mpsc::unbounded();
        assert!(check_and_download(&manager, &events).is_err());
        drop(events);
        let statuses: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(statuses
            .iter()
            .any(|s| matches!(s, UpdateStatus::Downloading { .. })));
        assert!(!statuses
            .iter()
            .any(|s| matches!(s, UpdateStatus::Ready { .. })));
        assert!(!dir.path().join("packages").join(&asset.FileName).exists());
    }

    #[test]
    #[ignore = "requires TANTRIX_UPDATE_FEED pointing to a locally packaged releases.*.json"]
    fn packaged_update_is_verified_and_remains_ready_offline() {
        let feed_path = std::path::PathBuf::from(std::env::var("TANTRIX_UPDATE_FEED").unwrap());
        let feed = fs::read(&feed_path).unwrap();
        let assets: VelopackAssetFeed = serde_json::from_slice(&feed).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let manager = installation(dir.path(), "0.0.1");
        fs::write(dir.path().join("releases.test.json"), &feed).unwrap();
        for asset in &assets.Assets {
            fs::copy(
                feed_path.parent().unwrap().join(&asset.FileName),
                dir.path().join(&asset.FileName),
            )
            .unwrap();
        }
        let (events, mut rx) = futures_channel::mpsc::unbounded();
        let downloaded = check_and_download(&manager, &events).unwrap().unwrap();
        assert!(dir
            .path()
            .join("packages")
            .join(&downloaded.FileName)
            .exists());
        assert_eq!(
            manager.get_update_pending_restart().unwrap().Version,
            downloaded.Version
        );
        while rx.try_recv().is_ok() {}
        fs::remove_file(dir.path().join("releases.test.json")).unwrap();
        assert!(check_and_download(&manager, &events).unwrap().is_some());
        drop(events);
        let statuses: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            statuses,
            [
                UpdateStatus::Checking,
                UpdateStatus::Ready {
                    version: downloaded.Version
                }
            ]
        );
    }
}
