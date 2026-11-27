//! Update check: the latest release published on GitHub (Latinu2020/ManageBIM).
//! The HTTPS request is made with `curl` (included in Windows 10 / 11) on a separate thread; nothing
//! about the user is sent - only the public release page is read.
use serde::Deserialize;
use std::sync::mpsc::{channel, Receiver};

pub const REPO: &str = "Latinu2020/ManageBIM";

#[derive(Clone, Debug, Default)]
pub struct UpdateInfo {
    /// the new version (without "v")
    pub version: String,
    /// the release page (release notes)
    pub page: String,
    /// the Windows installer (ManageBIM-Setup-<v>.exe), if any
    pub setup_url: String,
    pub setup_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

/// "0.27.32" < "0.28.0": numeric comparison by components; for the same numbers a release candidate
/// ("0.29.0-rc3") is older than the release ("0.29.0"), so rc users are offered the stable version.
pub fn newer(remote: &str, local: &str) -> bool {
    let pre = |s: &str| s.trim().trim_start_matches('v').contains('-');
    let parse = |s: &str| -> Vec<u64> {
        s.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parse(remote), parse(local));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    // same numbers: only a release replaces a release candidate
    pre(local) && !pre(remote)
}

/// The GitHub API response -> the update information, if it is newer than `local`.
pub fn parse_latest(json: &str, local: &str) -> Result<Option<UpdateInfo>, String> {
    let r: Release = serde_json::from_str(json).map_err(|e| format!("GitHub answer not understood: {e}"))?;
    if r.draft || r.prerelease || r.tag_name.is_empty() || !newer(&r.tag_name, local) {
        return Ok(None);
    }
    let setup = r.assets.iter().find(|a| a.name.to_ascii_lowercase().starts_with("managebim-setup") && a.name.to_ascii_lowercase().ends_with(".exe"));
    Ok(Some(UpdateInfo {
        version: r.tag_name.trim_start_matches('v').to_string(),
        page: r.html_url,
        setup_url: setup.map(|a| a.browser_download_url.clone()).unwrap_or_default(),
        setup_name: setup.map(|a| a.name.clone()).unwrap_or_default(),
    }))
}

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("curl is not available ({e})"))?;
    if !out.status.success() {
        return Err("GitHub did not answer - check the internet connection.".into());
    }
    Ok(out.stdout)
}

pub type CheckResult = Result<Option<UpdateInfo>, String>;

/// Checks on a separate thread; the result comes on the channel.
pub fn spawn_check() -> Receiver<CheckResult> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let r = curl(&["-s", "-S", "-f", "-L", "-m", "15", "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: ManageBIM", &url])
            .and_then(|b| parse_latest(&String::from_utf8_lossy(&b), env!("CARGO_PKG_VERSION")));
        let _ = tx.send(r);
    });
    rx
}

/// Downloads the installer to Downloads and starts it (the installer upgrades, keeping the folder and the settings).
pub fn spawn_download_and_run(info: UpdateInfo) -> Receiver<Result<std::path::PathBuf, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = (|| -> Result<std::path::PathBuf, String> {
            if info.setup_url.is_empty() {
                return Err("This release has no Windows installer - open the release page instead.".into());
            }
            let dir = std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("Downloads")).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
            let dest = dir.join(if info.setup_name.is_empty() { "ManageBIM-Setup.exe".to_string() } else { info.setup_name.clone() });
            let d = dest.to_string_lossy().to_string();
            curl(&["-s", "-S", "-f", "-L", "-m", "900", "-o", &d, &info.setup_url])?;
            std::process::Command::new(&dest).spawn().map_err(|e| format!("The installer was downloaded to {d} but could not be started: {e}"))?;
            Ok(dest)
        })();
        let _ = tx.send(r);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order() {
        assert!(newer("v0.28.0", "0.27.32"));
        assert!(newer("0.27.33", "0.27.32"));
        assert!(!newer("v0.27.32", "0.27.32"));
        assert!(!newer("0.27.9", "0.27.32"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.28.0-rc1", "0.28.0"));
        // a release candidate is older than its release
        assert!(newer("v0.29.0", "0.29.0-rc3"));
        assert!(!newer("v0.29.0-rc3", "0.29.0-rc3"));
    }

    #[test]
    fn parses_release_and_finds_the_setup() {
        let j = r#"{"tag_name":"v9.0.0","html_url":"https://github.com/Latinu2020/ManageBIM/releases/tag/v9.0.0","draft":false,"prerelease":false,
            "assets":[{"name":"ManageBIM-9.0.0-x86_64.AppImage","browser_download_url":"https://x/a"},{"name":"ManageBIM-Setup-9.0.0.exe","browser_download_url":"https://x/setup.exe"}]}"#;
        let u = parse_latest(j, "0.28.0").unwrap().expect("newer");
        assert_eq!(u.version, "9.0.0");
        assert_eq!(u.setup_url, "https://x/setup.exe");
        assert!(parse_latest(j, "9.0.0").unwrap().is_none());
    }
}
