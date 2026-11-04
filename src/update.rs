//! Self-update from the project's GitHub releases.

use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    platform,
    tasks::{ProgressSnapshot, TaskCompletionData, TaskOutcome, TaskReporter},
};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECKSUMS_ASSET: &str = "SHA256SUMS";
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/GeekRhapsody/PadCommander/releases/latest";
// GitHub rejects API requests without a User-Agent.
const USER_AGENT: &str = concat!("PadCommander/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Update {
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub checksums_url: String,
    pub size: u64,
    /// The file this update replaces, found before anything is renamed.
    pub target: PathBuf,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Returns the latest release when it is newer than this build.
pub fn check_latest() -> Result<Option<Update>, String> {
    let (target, asset_suffix) = platform::update_target()
        .map_err(|error| format!("Could not find the PadCommander program file: {error}"))?;
    let body =
        get_text(LATEST_RELEASE_URL).map_err(|error| format!("Could not reach GitHub: {error}"))?;
    let release: GithubRelease = serde_json::from_str(&body)
        .map_err(|error| format!("Unexpected reply from GitHub: {error}"))?;
    newer_update(release, asset_suffix, target)
}

fn newer_update(
    release: GithubRelease,
    asset_suffix: &str,
    target: PathBuf,
) -> Result<Option<Update>, String> {
    let version = release.tag_name.trim_start_matches('v').to_owned();
    if !is_newer(&version, CURRENT_VERSION) {
        return Ok(None);
    }

    let assets = &release.assets;
    let Some(asset) = assets.iter().find(|a| a.name.ends_with(asset_suffix)) else {
        return Err(format!(
            "Version {version} has no download for this system."
        ));
    };
    let Some(checksums) = assets.iter().find(|a| a.name == CHECKSUMS_ASSET) else {
        return Err(format!(
            "Version {version} has no {CHECKSUMS_ASSET} file, so its download cannot be verified."
        ));
    };
    Ok(Some(Update {
        version,
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        checksums_url: checksums.browser_download_url.clone(),
        size: asset.size,
        target,
    }))
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| {
        version
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Downloads the update and swaps it in for the program file. The running app keeps working until it restarts.
pub fn install(reporter: TaskReporter, update: Update) -> TaskOutcome {
    let expected_checksum = match get_text(&update.checksums_url) {
        Ok(sums) => expected_checksum(&sums, &update.asset_name),
        Err(error) => {
            return TaskOutcome::Failed(format!("Could not download {CHECKSUMS_ASSET}: {error}"));
        }
    };
    let Some(expected_checksum) = expected_checksum else {
        return TaskOutcome::Failed(format!(
            "{} is not listed in {CHECKSUMS_ASSET}.",
            update.asset_name
        ));
    };
    let downloaded = match download(&reporter, &update) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return TaskOutcome::Cancelled,
        Err(error) => return TaskOutcome::Failed(format!("Download failed: {error}")),
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some("Installing".to_owned()),
        bytes_done: update.size,
        total_bytes: Some(update.size),
        ..ProgressSnapshot::default()
    });
    let checksum: String = Sha256::digest(&downloaded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !checksum.eq_ignore_ascii_case(&expected_checksum) {
        return TaskOutcome::Failed(
            "The download does not match its published checksum. Nothing was changed.".to_owned(),
        );
    }
    let executable = match unpack(&update.download_url, downloaded) {
        Ok(executable) => executable,
        Err(error) => return TaskOutcome::Failed(format!("Could not unpack the update: {error}")),
    };
    if let Err(error) = replace_executable(&update.target, &executable) {
        return TaskOutcome::Failed(format!(
            "Could not replace {}: {error}",
            update.target.display()
        ));
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: 1,
        bytes_done: update.size,
    })
}

fn get_text(url: &str) -> Result<String, ureq::Error> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()?
        .body_mut()
        .read_to_string()
}

/// Reads the hash listed for `asset_name` in `sha256sum` output: "<hash>  <name>", or "<hash> *<name>" in binary mode.
fn expected_checksum(sums: &str, asset_name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim_start().trim_start_matches('*') == asset_name).then(|| hash.to_owned())
    })
}

/// `Ok(None)` means the user cancelled.
fn download(reporter: &TaskReporter, update: &Update) -> Result<Option<Vec<u8>>, String> {
    let response = ureq::get(&update.download_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::with_capacity(usize::try_from(update.size).unwrap_or_default());
    let mut buffer = vec![0u8; 64 * 1024];
    let current_file = Some(format!("Downloading version {}", update.version));

    loop {
        if reporter.is_cancelled() {
            return Ok(None);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(Some(bytes));
        }
        bytes.extend_from_slice(&buffer[..read]);
        reporter.progress(ProgressSnapshot {
            current_file: current_file.clone(),
            bytes_done: bytes.len() as u64,
            total_bytes: Some(update.size),
            ..ProgressSnapshot::default()
        });
    }
}

/// Pulls the program file out of the release asset the release workflow builds.
fn unpack(asset_url: &str, downloaded: Vec<u8>) -> io::Result<Vec<u8>> {
    let mut executable = Vec::new();
    if asset_url.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(Cursor::new(downloaded))?;
        archive
            .by_name("pad-commander.exe")?
            .read_to_end(&mut executable)?;
        return Ok(executable);
    }
    if !asset_url.ends_with(".tar.gz") {
        return Ok(downloaded);
    }

    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(downloaded)));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()? == Path::new("pad-commander") {
            entry.read_to_end(&mut executable)?;
            return Ok(executable);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "pad-commander is missing from the download",
    ))
}

/// Windows cannot overwrite a running program but can rename it, so the old file moves aside
/// and is removed on the next start.
fn replace_executable(target: &Path, executable: &[u8]) -> io::Result<()> {
    let new_path = with_suffix(target, ".new");
    let old_path = with_suffix(target, ".old");
    fs::write(&new_path, executable)?;
    platform::make_executable(&new_path)?;
    let _ = fs::remove_file(&old_path);
    fs::rename(target, &old_path)?;
    if let Err(error) = fs::rename(&new_path, target) {
        let _ = fs::rename(&old_path, target);
        return Err(error);
    }
    // Succeeds on Linux; on Windows the file is still in use until the app exits.
    let _ = fs::remove_file(&old_path);
    Ok(())
}

/// Removes the old program file a Windows update had to leave behind.
pub fn remove_leftover() {
    if let Ok((target, _)) = platform::update_target() {
        let _ = fs::remove_file(with_suffix(&target, ".old"));
    }
}

pub fn restart(target: &Path) -> io::Result<()> {
    Command::new(target)
        .args(env::args_os().skip(1))
        .spawn()
        .map(drop)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn release(tag: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.to_owned(),
            assets: vec![
                GithubAsset {
                    name: format!("pad-commander-{tag}-windows-x86_64.zip"),
                    browser_download_url: "https://example.test/win.zip".to_owned(),
                    size: 10,
                },
                GithubAsset {
                    name: format!("pad-commander-{tag}-linux-x86_64.AppImage"),
                    browser_download_url: "https://example.test/linux.AppImage".to_owned(),
                    size: 20,
                },
                GithubAsset {
                    name: "SHA256SUMS".to_owned(),
                    browser_download_url: "https://example.test/SHA256SUMS".to_owned(),
                    size: 1,
                },
            ],
        }
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("2.0.10", "2.0.9"));
        assert!(is_newer("2.1", "2.0.1"));
        assert!(is_newer("3.0.0", "2.9.9"));
        assert!(!is_newer("2.0.1", "2.0.1"));
        assert!(!is_newer("1.9.9", "2.0.0"));
        assert!(!is_newer("2.1.0-beta", "2.0.0"));
    }

    #[test]
    fn picks_the_asset_for_this_install() {
        let update = newer_update(
            release("v99.0.0"),
            "-linux-x86_64.AppImage",
            PathBuf::from("app"),
        )
        .unwrap()
        .unwrap();

        assert_eq!(update.version, "99.0.0");
        assert_eq!(
            update.asset_name,
            "pad-commander-v99.0.0-linux-x86_64.AppImage"
        );
        assert_eq!(update.download_url, "https://example.test/linux.AppImage");
        assert_eq!(update.checksums_url, "https://example.test/SHA256SUMS");
        assert_eq!(update.size, 20);
    }

    #[test]
    fn current_release_is_not_an_update() {
        let tag = format!("v{CURRENT_VERSION}");
        let update = newer_update(release(&tag), "-windows-x86_64.zip", PathBuf::new());

        assert_eq!(update, Ok(None));
    }

    #[test]
    fn newer_release_without_matching_asset_is_an_error() {
        let update = newer_update(release("v99.0.0"), "-linux-x86_64.tar.gz", PathBuf::new());

        assert!(update.is_err());
    }

    #[test]
    fn newer_release_without_checksums_is_an_error() {
        let mut release = release("v99.0.0");
        release.assets.retain(|asset| asset.name != "SHA256SUMS");

        let update = newer_update(release, "-windows-x86_64.zip", PathBuf::new());

        assert!(update.unwrap_err().contains("SHA256SUMS"));
    }

    #[test]
    fn reads_checksum_for_asset_from_sha256sum_output() {
        let sums = "aaa  pad-commander-v3.0.0-linux-x86_64.tar.gz\nbbb *pad-commander-v3.0.0-windows-x86_64.zip\n";

        assert_eq!(
            expected_checksum(sums, "pad-commander-v3.0.0-linux-x86_64.tar.gz"),
            Some("aaa".to_owned())
        );
        assert_eq!(
            expected_checksum(sums, "pad-commander-v3.0.0-windows-x86_64.zip"),
            Some("bbb".to_owned())
        );
        assert_eq!(expected_checksum(sums, "pad-commander.AppImage"), None);
    }

    #[test]
    fn unpacks_program_from_zip_and_tarball() {
        let mut zip_bytes = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut zip_bytes);
        writer
            .start_file(
                "pad-commander.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"windows build").unwrap();
        writer.finish().unwrap();

        let mut tar_builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(11);
        header.set_mode(0o755);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, "pad-commander", &b"linux build"[..])
            .unwrap();
        let tar_bytes = tar_builder.into_inner().unwrap().finish().unwrap();

        assert_eq!(
            unpack("x.zip", zip_bytes.into_inner()).unwrap(),
            b"windows build"
        );
        assert_eq!(unpack("x.tar.gz", tar_bytes).unwrap(), b"linux build");
        assert_eq!(unpack("x.AppImage", b"image".to_vec()).unwrap(), b"image");
    }

    #[test]
    fn replaces_program_file_in_place() {
        let dir = env::temp_dir().join(format!("pad-commander-update-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("pad-commander");
        fs::write(&target, b"old").unwrap();

        replace_executable(&target, b"new").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!with_suffix(&target, ".new").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
