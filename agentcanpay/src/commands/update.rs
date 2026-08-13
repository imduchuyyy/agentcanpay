use std::{env, ffi::OsStr, path::Path, process::Command};

use super::CommandError;
use crate::{cli::UpdateArgs, output::Output};

const REPO: &str = "imduchuyyy/agentcanpay";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Replaces this binary with the newest published release.
///
/// The work is done by the install script rather than in this process, for
/// three reasons: it is the same code path every new user exercises, so it
/// cannot rot separately; it keeps archive handling out of the binary that
/// holds the recovery phrase; and a child process can replace a file its
/// parent is executing, which on Windows is the only way to do it at all.
pub async fn run(args: &UpdateArgs, out: &Output) -> Result<(), CommandError> {
    let exe = env::current_exe().map_err(|e| CommandError::UpdateFailed(e.to_string()))?;
    let exe = exe.canonicalize().unwrap_or(exe);

    let latest = latest_version().await?;
    let available = is_newer(&latest, CURRENT);

    if args.check || !available {
        out.update(CURRENT, &latest, false, &exe);
        return Ok(());
    }

    // A binary under a package manager's prefix is that manager's to
    // replace: overwriting it leaves its records describing a file that is
    // no longer there, and the next upgrade silently reverts this one.
    if let Some(manager) = managed_by(&exe) {
        return Err(CommandError::UpdateManaged(manager.to_owned()));
    }

    let dir = exe
        .parent()
        .ok_or_else(|| CommandError::UpdateFailed("binary has no parent directory".into()))?;

    out.note(&format!("updating {CURRENT} -> {latest}"));
    run_installer(&latest, dir).await?;

    out.update(CURRENT, &latest, true, &exe);
    Ok(())
}

/// Resolves the newest release through the redirect on `/releases/latest`.
///
/// Reading the `Location` header rather than calling the REST API keeps
/// this free of tokens and of the 60-per-hour anonymous rate limit, which
/// an agent sharing an IP with others would otherwise hit.
async fn latest_version() -> Result<String, CommandError> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CommandError::UpdateCheck(e.to_string()))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| CommandError::UpdateCheck(e.to_string()))?;

    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| CommandError::UpdateCheck("GitHub did not name a latest release".into()))?;

    Ok(tag_from_location(location)?
        .trim_start_matches('v')
        .to_owned())
}

/// Pulls the tag out of the redirect target.
///
/// The `/releases/tag/` prefix is required rather than assumed: a
/// repository with no releases at all redirects to the plain `/releases`
/// page, and taking the last segment there would report "releases" as the
/// newest version.
fn tag_from_location(location: &str) -> Result<&str, CommandError> {
    location
        .split_once("/releases/tag/")
        .map(|(_, tag)| tag.trim_end_matches('/'))
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| {
            CommandError::UpdateCheck(format!(
                "no published release found (redirected to {location})"
            ))
        })
}

/// Downloads the install script for the target release and runs it.
///
/// The script is fetched at the release tag, not from the default branch:
/// the installer that runs is the one that shipped with the version being
/// installed, so a change to the layout of a future release cannot break
/// an update into an older one.
async fn run_installer(version: &str, bin_dir: &Path) -> Result<(), CommandError> {
    let script = if cfg!(windows) {
        "install.ps1"
    } else {
        "install.sh"
    };
    let url = format!("https://raw.githubusercontent.com/{REPO}/v{version}/{script}");

    let body = reqwest::get(&url)
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| CommandError::UpdateFailed(format!("could not fetch {script}: {e}")))?
        .text()
        .await
        .map_err(|e| CommandError::UpdateFailed(e.to_string()))?;

    let path = env::temp_dir().join(format!("agentcanpay-{version}-{script}"));
    std::fs::write(&path, body).map_err(|e| CommandError::UpdateFailed(e.to_string()))?;

    let status = installer_command(&path)
        .env("AGENTCANPAY_VERSION", version)
        .env("AGENTCANPAY_BIN_DIR", bin_dir)
        .status()
        .map_err(|e| CommandError::UpdateFailed(format!("could not run {script}: {e}")))?;

    // Best-effort: a leftover script in the temp dir is harmless, and a
    // failure to remove it must not mask a successful update.
    let _ = std::fs::remove_file(&path);

    if !status.success() {
        return Err(CommandError::UpdateFailed(format!(
            "{script} exited with {status}; the existing binary is untouched"
        )));
    }
    Ok(())
}

/// The script is passed to an interpreter rather than executed directly,
/// so a temp directory mounted noexec cannot block an update.
fn installer_command(path: &Path) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(path)
            .arg("-Quiet");
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg(path).arg("--quiet");
        cmd
    }
}

/// Names the package manager owning this path, if any.
///
/// Matched on directory components rather than substrings, so a wallet
/// installed at `~/.cargo-backups/` is not mistaken for a cargo install.
fn managed_by(exe: &Path) -> Option<&'static str> {
    let has = |name: &str| exe.components().any(|c| c.as_os_str() == OsStr::new(name));

    if has("Cellar") || has("homebrew") || has("linuxbrew") {
        Some("Homebrew")
    } else if has("store") && has("nix") {
        Some("Nix")
    } else if has("node_modules") || has("_npx") {
        Some("npm")
    } else if has(".cargo") {
        Some("cargo")
    } else {
        None
    }
}

/// Compares two dotted versions numerically.
///
/// A string comparison would rank 0.10.0 below 0.9.0 and leave every user
/// stranded on the older release, so the parts are compared as numbers. An
/// unparseable version is treated as "nothing newer": refusing to act on a
/// tag we do not understand is safer than replacing a working binary.
fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        v.split('.')
            .map(|p| p.split(['-', '+']).next().unwrap_or(p).parse().ok())
            .collect()
    };
    match (parts(latest), parts(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_newer, managed_by, tag_from_location};
    use std::path::Path;

    #[test]
    fn reads_the_tag_out_of_a_release_redirect() {
        let tag = tag_from_location("https://github.com/a/b/releases/tag/v0.2.0")
            .expect("a release redirect names a tag");
        assert_eq!(tag, "v0.2.0");
    }

    /// A repository with no releases redirects to the listing page, which
    /// must read as "nothing published", not as a version called releases.
    #[test]
    fn a_repository_without_releases_is_an_error() {
        assert!(tag_from_location("https://github.com/a/b/releases").is_err());
    }

    #[test]
    fn compares_versions_numerically_not_lexically() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn same_or_older_is_not_an_update() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    /// A tag we cannot parse must never trigger a replacement.
    #[test]
    fn unparseable_versions_never_update() {
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }

    #[test]
    fn recognises_package_manager_prefixes() {
        assert_eq!(
            managed_by(Path::new("/opt/homebrew/bin/agentcanpay")),
            Some("Homebrew")
        );
        assert_eq!(
            managed_by(Path::new("/nix/store/abc-agentcanpay/bin/agentcanpay")),
            Some("Nix")
        );
        assert_eq!(
            managed_by(Path::new("/home/a/.cargo/bin/agentcanpay")),
            Some("cargo")
        );
    }

    /// The install script's own default location must stay updatable.
    #[test]
    fn leaves_self_installed_paths_alone() {
        assert_eq!(
            managed_by(Path::new("/home/a/.agentcanpay/bin/agentcanpay")),
            None
        );
        assert_eq!(managed_by(Path::new("/usr/local/bin/agentcanpay")), None);
    }
}
