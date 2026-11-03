use crate::{
    state::{FileEntry, FileEntryFlags, FileEntryKind},
    tasks::{ProgressSnapshot, TaskCompletionData, TaskOutcome, TaskReporter},
};

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use suppaftp::{FtpError, RustlsConnector, RustlsFtpStream, list::File, rustls, types::FileType};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FtpServer {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub path: String,
    pub secure: bool,
    #[serde(default)]
    pub saved_password: Option<String>,
}

impl FtpServer {
    pub fn is_anonymous(&self) -> bool {
        self.username.is_empty()
    }

    pub fn root_location(&self) -> String {
        let scheme = if self.secure { "ftps" } else { "ftp" };
        format!("{scheme}://{}:{}", self.host, self.port)
    }

    pub fn start_location(&self) -> String {
        format!("{}{}", self.root_location(), self.path)
    }

    pub fn label(&self) -> String {
        let scheme = if self.secure { "ftps" } else { "ftp" };
        let user = if self.is_anonymous() {
            String::new()
        } else {
            format!("{}@", self.username)
        };
        format!("{scheme}://{user}{}:{}{}", self.host, self.port, self.path)
    }
}

pub fn parse_address(input: &str) -> Result<FtpServer, String> {
    let input = input.trim();
    let (secure, rest) = match input.strip_prefix("ftps://") {
        Some(rest) => (true, rest),
        None => (false, input.strip_prefix("ftp://").unwrap_or(input)),
    };
    let (authority, path) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, "/"),
    };
    let (username, host_port) = authority.rsplit_once('@').unwrap_or(("", authority));
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| format!("Invalid port '{port}'."))?,
        ),
        None => (host_port, 21),
    };
    if host.is_empty() {
        return Err("Server address needs a host name.".to_owned());
    }

    let path = path.trim_end_matches('/');
    Ok(FtpServer {
        host: host.to_owned(),
        port,
        username: username.to_owned(),
        path: if path.is_empty() { "/" } else { path }.to_owned(),
        secure,
        saved_password: None,
    })
}

pub fn is_ftp_location(location: &str) -> bool {
    location.starts_with("ftp://") || location.starts_with("ftps://")
}

/// Splits `ftp://host:port/a/b` into `("ftp://host:port", "/a/b")`.
pub fn split_location(location: &str) -> (&str, &str) {
    let authority_start = location.find("://").map_or(0, |index| index + 3);
    match location[authority_start..].find('/') {
        Some(index) => location.split_at(authority_start + index),
        None => (location, "/"),
    }
}

pub fn parent_location(location: &str) -> Option<String> {
    let (root, path) = split_location(location);
    (path != "/").then(|| format!("{root}{}", remote_parent(path)))
}

pub fn child_location(location: &str, name: &str) -> String {
    let (root, path) = split_location(location);
    format!("{root}{}", join_remote(path, name))
}

fn remote_parent(path: &str) -> &str {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((parent, _)) => parent,
    }
}

fn join_remote(directory: &str, name: &str) -> String {
    format!("{}/{name}", directory.trim_end_matches('/'))
}

/// A pane's FTP session. Commands are serialized through the mutex and run on background threads.
pub struct FtpConnection {
    pub server: FtpServer,
    pub password: String,
    stream: Mutex<Option<RustlsFtpStream>>,
}

impl FtpConnection {
    pub fn new(server: FtpServer, password: String) -> Self {
        Self {
            server,
            password,
            stream: Mutex::new(None),
        }
    }

    pub fn run<T>(
        &self,
        operation: impl Fn(&mut RustlsFtpStream) -> Result<T, FtpError>,
    ) -> Result<T, String> {
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| "FTP connection is unavailable.".to_owned())?;
        if let Some(stream) = guard.as_mut() {
            match operation(stream) {
                // Servers drop idle control connections; reconnect once and retry.
                Err(FtpError::ConnectionError(_) | FtpError::BadResponse) => {}
                result => return result.map_err(|error| error.to_string()),
            }
        }

        let stream =
            guard.insert(connect(&self.server, &self.password).map_err(|error| error.to_string())?);
        operation(stream).map_err(|error| error.to_string())
    }
}

pub fn connect(server: &FtpServer, password: &str) -> Result<RustlsFtpStream, FtpError> {
    let address = (server.host.as_str(), server.port)
        .to_socket_addrs()
        .map_err(FtpError::ConnectionError)?
        .next()
        .ok_or_else(|| {
            FtpError::ConnectionError(io::Error::new(
                io::ErrorKind::NotFound,
                format!("cannot resolve {}", server.host),
            ))
        })?;
    let mut stream = RustlsFtpStream::connect_timeout(address, NETWORK_TIMEOUT)?
        .passive_stream_builder(|address| {
            let stream = TcpStream::connect_timeout(&address, NETWORK_TIMEOUT)
                .map_err(FtpError::ConnectionError)?;
            apply_timeouts(&stream).map_err(FtpError::ConnectionError)?;
            Ok(stream)
        });
    apply_timeouts(stream.get_ref()).map_err(FtpError::ConnectionError)?;

    if server.secure {
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        stream = stream.into_secure(RustlsConnector::from(Arc::new(config)), &server.host)?;
    }

    if server.is_anonymous() {
        stream.login("anonymous", "anonymous@")?;
    } else {
        stream.login(server.username.as_str(), password)?;
    }
    stream.transfer_type(FileType::Binary)?;
    Ok(stream)
}

fn apply_timeouts(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(NETWORK_TIMEOUT))?;
    stream.set_write_timeout(Some(NETWORK_TIMEOUT))
}

/// Lists a remote folder and appends a `..` entry that leads to `parent_location`.
pub fn list(
    stream: &mut RustlsFtpStream,
    location: &str,
    parent_location: &str,
) -> Result<Vec<FileEntry>, FtpError> {
    let files = list_files(stream, split_location(location).1)?;
    let parent = FileEntry {
        display_name: "..".to_owned(),
        virtual_path: parent_location.to_owned(),
        kind: FileEntryKind::Parent,
        size: None,
        modified: None,
        flags: FileEntryFlags::default(),
    };
    Ok(files
        .iter()
        .map(|file| FileEntry {
            display_name: file.name().to_owned(),
            virtual_path: child_location(location, file.name()),
            kind: if file.is_directory() {
                FileEntryKind::Directory
            } else {
                FileEntryKind::FtpItem
            },
            size: (!file.is_directory()).then_some(file.size() as u64),
            modified: Some(file.modified()),
            flags: FileEntryFlags {
                hidden: file.name().starts_with('.'),
                read_only: false,
            },
        })
        .chain([parent])
        .collect())
}

fn list_files(stream: &mut RustlsFtpStream, path: &str) -> Result<Vec<File>, FtpError> {
    let files: Vec<File> = match stream.mlsd(Some(path)) {
        Ok(lines) => lines
            .iter()
            .filter_map(|line| File::from_mlsx_line(line).ok())
            .collect(),
        // MLSD is optional (RFC 3659); fall back to the classic LIST format.
        Err(_) => stream
            .list(Some(path))?
            .iter()
            .filter_map(|line| File::from_str(line).ok())
            .collect(),
    };
    Ok(files
        .into_iter()
        .filter(|file| file.name() != "." && file.name() != "..")
        .collect())
}

pub fn create_folder(stream: &mut RustlsFtpStream, location: &str) -> Result<(), FtpError> {
    stream.mkdir(split_location(location).1)
}

pub fn create_empty_file(stream: &mut RustlsFtpStream, location: &str) -> Result<(), FtpError> {
    stream
        .put_file(split_location(location).1, &mut io::empty())
        .map(|_| ())
}

pub fn rename(
    stream: &mut RustlsFtpStream,
    location: &str,
    new_name: &str,
) -> Result<(), FtpError> {
    let from = split_location(location).1;
    stream.rename(from, join_remote(remote_parent(from), new_name).as_str())
}

fn remove_tree(
    stream: &mut RustlsFtpStream,
    path: &str,
    is_directory: bool,
) -> Result<(), FtpError> {
    if !is_directory {
        return stream.rm(path);
    }

    for file in list_files(stream, path)? {
        remove_tree(stream, &join_remote(path, file.name()), file.is_directory())?;
    }
    stream.rmdir(path)
}

/// A remote item picked in a pane: its full location, whether it is a directory, and its size.
pub type RemoteSource = (String, bool, Option<u64>);

pub fn remote_sources(entries: &[FileEntry]) -> Vec<RemoteSource> {
    entries
        .iter()
        .map(|entry| {
            (
                entry.virtual_path.clone(),
                entry.kind == FileEntryKind::Directory,
                entry.size,
            )
        })
        .collect()
}

pub fn delete(
    reporter: TaskReporter,
    server: FtpServer,
    password: String,
    sources: Vec<RemoteSource>,
) -> Result<TaskOutcome, String> {
    let mut stream = connect(&server, &password).map_err(|error| error.to_string())?;
    let mut snapshot = ProgressSnapshot {
        total_items: Some(sources.len() as u64),
        ..ProgressSnapshot::default()
    };

    for (location, is_directory, _) in &sources {
        if reporter.is_cancelled() {
            return Ok(TaskOutcome::Cancelled);
        }
        snapshot.current_file = Some(location.clone());
        reporter.progress(snapshot.clone());
        remove_tree(&mut stream, split_location(location).1, *is_directory)
            .map_err(|error| format!("Cannot delete {location}: {error}"))?;
        snapshot.items_done += 1;
    }

    let _ = stream.quit();
    Ok(TaskOutcome::Succeeded(TaskCompletionData {
        item_count: sources.len(),
        bytes_done: 0,
    }))
}

pub fn upload(
    reporter: TaskReporter,
    server: FtpServer,
    password: String,
    sources: Vec<PathBuf>,
    target_location: String,
    delete_sources: bool,
) -> Result<TaskOutcome, String> {
    let target_directory = split_location(&target_location).1;
    let mut items = Vec::new();
    for source in &sources {
        let name = source.file_name().unwrap_or_default().to_string_lossy();
        plan_upload(source, join_remote(target_directory, &name), &mut items)
            .map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
    }

    let mut stream = connect(&server, &password).map_err(|error| error.to_string())?;
    let mut snapshot = ProgressSnapshot {
        total_bytes: Some(items.iter().filter_map(|item| item.2).sum()),
        total_items: Some(items.len() as u64),
        ..ProgressSnapshot::default()
    };

    for (local, remote, size) in &items {
        if reporter.is_cancelled() {
            return Ok(TaskOutcome::Cancelled);
        }
        snapshot.current_file = Some(remote.clone());
        reporter.progress(snapshot.clone());

        if size.is_none() {
            // The folder may already exist when the user chose to overwrite.
            let _ = stream.mkdir(remote);
            snapshot.items_done += 1;
            continue;
        }

        let mut file = fs::File::open(local)
            .map_err(|error| format!("Cannot open {}: {error}", local.display()))?;
        let mut data = stream
            .put_with_stream(remote)
            .map_err(|error| format!("Cannot upload {remote}: {error}"))?;
        let completed = copy_with_progress(&mut file, &mut data, &reporter, &mut snapshot)
            .map_err(|error| format!("Upload of {remote} failed: {error}"))?;
        if !completed {
            return Ok(TaskOutcome::Cancelled);
        }
        stream
            .finalize_put_stream(data)
            .map_err(|error| format!("Upload of {remote} failed: {error}"))?;
        snapshot.items_done += 1;
    }
    let _ = stream.quit();

    if delete_sources {
        for source in &sources {
            let removed = if source.is_dir() {
                fs::remove_dir_all(source)
            } else {
                fs::remove_file(source)
            };
            removed.map_err(|error| {
                format!("Uploaded, but cannot remove {}: {error}", source.display())
            })?;
        }
    }

    Ok(TaskOutcome::Succeeded(TaskCompletionData {
        item_count: sources.len(),
        bytes_done: snapshot.bytes_done,
    }))
}

fn plan_upload(
    source: &Path,
    remote: String,
    items: &mut Vec<(PathBuf, String, Option<u64>)>,
) -> io::Result<()> {
    let metadata = fs::metadata(source)?;
    if !metadata.is_dir() {
        items.push((source.to_path_buf(), remote, Some(metadata.len())));
        return Ok(());
    }

    items.push((source.to_path_buf(), remote.clone(), None));
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        plan_upload(
            &entry.path(),
            join_remote(&remote, &name.to_string_lossy()),
            items,
        )?;
    }
    Ok(())
}

pub fn download(
    reporter: TaskReporter,
    server: FtpServer,
    password: String,
    sources: Vec<RemoteSource>,
    target_directory: PathBuf,
    delete_sources: bool,
) -> Result<TaskOutcome, String> {
    let mut stream = connect(&server, &password).map_err(|error| error.to_string())?;
    let mut items = Vec::new();
    for (location, is_directory, size) in &sources {
        let path = split_location(location).1;
        let name = path.rsplit('/').next().unwrap_or(path);
        plan_download(
            &mut stream,
            path.to_owned(),
            target_directory.join(name),
            *is_directory,
            size.unwrap_or(0),
            &mut items,
        )
        .map_err(|error| format!("Cannot list {location}: {error}"))?;
    }

    let mut snapshot = ProgressSnapshot {
        total_bytes: Some(items.iter().filter_map(|item| item.2).sum()),
        total_items: Some(items.len() as u64),
        ..ProgressSnapshot::default()
    };

    for (remote, local, size) in &items {
        if reporter.is_cancelled() {
            return Ok(TaskOutcome::Cancelled);
        }
        snapshot.current_file = Some(remote.clone());
        reporter.progress(snapshot.clone());

        if size.is_none() {
            fs::create_dir_all(local)
                .map_err(|error| format!("Cannot create {}: {error}", local.display()))?;
            snapshot.items_done += 1;
            continue;
        }

        let mut file = fs::File::create(local)
            .map_err(|error| format!("Cannot create {}: {error}", local.display()))?;
        let mut data = stream
            .retr_as_stream(remote)
            .map_err(|error| format!("Cannot download {remote}: {error}"))?;
        let completed = copy_with_progress(&mut data, &mut file, &reporter, &mut snapshot)
            .map_err(|error| format!("Download of {remote} failed: {error}"))?;
        if !completed {
            drop(file);
            let _ = fs::remove_file(local);
            return Ok(TaskOutcome::Cancelled);
        }
        stream
            .finalize_retr_stream(data)
            .map_err(|error| format!("Download of {remote} failed: {error}"))?;
        snapshot.items_done += 1;
    }

    if delete_sources {
        for (location, is_directory, _) in &sources {
            remove_tree(&mut stream, split_location(location).1, *is_directory)
                .map_err(|error| format!("Downloaded, but cannot remove {location}: {error}"))?;
        }
    }
    let _ = stream.quit();

    Ok(TaskOutcome::Succeeded(TaskCompletionData {
        item_count: sources.len(),
        bytes_done: snapshot.bytes_done,
    }))
}

fn plan_download(
    stream: &mut RustlsFtpStream,
    remote: String,
    local: PathBuf,
    is_directory: bool,
    size: u64,
    items: &mut Vec<(String, PathBuf, Option<u64>)>,
) -> Result<(), FtpError> {
    if !is_directory {
        items.push((remote, local, Some(size)));
        return Ok(());
    }

    items.push((remote.clone(), local.clone(), None));
    for file in list_files(stream, &remote)? {
        plan_download(
            stream,
            join_remote(&remote, file.name()),
            local.join(file.name()),
            file.is_directory(),
            file.size() as u64,
            items,
        )?;
    }
    Ok(())
}

/// Returns `Ok(false)` when the task was cancelled part-way.
fn copy_with_progress(
    reader: &mut impl Read,
    writer: &mut impl Write,
    reporter: &TaskReporter,
    snapshot: &mut ProgressSnapshot,
) -> io::Result<bool> {
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if reporter.is_cancelled() {
            return Ok(false);
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(true);
        }
        writer.write_all(&buffer[..read])?;
        snapshot.bytes_done += read as u64;
        reporter.progress(snapshot.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_address() {
        let server = parse_address("ftps://deck@nas.local:2121/media/games/").expect("parse");

        assert_eq!(server.host, "nas.local");
        assert_eq!(server.port, 2121);
        assert_eq!(server.username, "deck");
        assert_eq!(server.path, "/media/games");
        assert!(server.secure);
        assert_eq!(server.start_location(), "ftps://nas.local:2121/media/games");
    }

    #[test]
    fn bare_host_is_anonymous_ftp_on_default_port() {
        let server = parse_address("ftp.example.com").expect("parse");

        assert!(server.is_anonymous());
        assert!(!server.secure);
        assert_eq!(server.port, 21);
        assert_eq!(server.path, "/");
        assert_eq!(server.start_location(), "ftp://ftp.example.com:21/");
    }

    #[test]
    fn rejects_missing_host_and_bad_port() {
        assert!(parse_address("ftp://user@/files").is_err());
        assert!(parse_address("ftp://host:99999").is_err());
    }

    #[test]
    fn location_helpers_walk_remote_paths() {
        let location = "ftp://host:21/pub/files";

        assert_eq!(split_location(location), ("ftp://host:21", "/pub/files"));
        assert_eq!(
            parent_location(location).as_deref(),
            Some("ftp://host:21/pub")
        );
        assert_eq!(
            parent_location("ftp://host:21/pub").as_deref(),
            Some("ftp://host:21/")
        );
        assert_eq!(parent_location("ftp://host:21/"), None);
        assert_eq!(child_location("ftp://host:21/", "a"), "ftp://host:21/a");
        assert_eq!(
            child_location(location, "a.txt"),
            "ftp://host:21/pub/files/a.txt"
        );
        assert!(is_ftp_location("ftps://host:990/"));
        assert!(!is_ftp_location(r"C:\ftp"));
    }

    #[test]
    fn server_label_hides_password_and_shows_user() {
        let mut server = parse_address("ftp://deck@nas:21/").expect("parse");
        server.saved_password = Some("sealed".to_owned());

        assert_eq!(server.label(), "ftp://deck@nas:21/");
    }
}
