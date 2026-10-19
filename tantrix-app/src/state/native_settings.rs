//! Native preferences and private reconnect identities, stored per server.

use std::{fs, path::PathBuf};

pub fn settings_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    return crate::platform::android::settings_dir();
    #[cfg(not(target_os = "android"))]
    dirs::config_dir()
        .map(|path| path.join("tantrix"))
        .ok_or_else(|| "Could not locate the application settings directory.".into())
}

pub fn load(name: &str) -> Option<String> {
    fs::read_to_string(settings_dir().ok()?.join(name)).ok()
}

pub fn save(name: &str, value: &str) -> Result<(), String> {
    let directory = settings_dir()?;
    fs::create_dir_all(&directory).map_err(|error| format!("Could not save settings: {error}"))?;
    let path = directory.join(name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options
        .open(path)
        .and_then(|mut file| file.write_all(value.as_bytes()))
        .map_err(|error| format!("Could not save settings: {error}"))
}
