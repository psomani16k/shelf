use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};

use shelf_core::timeutil::Time;

const PREFIX: &str = "shelf-";
const SUFFIX: &str = ".log";

/// One file per app start, `shelf-<start time>.log`, never renamed. The time is RFC 3339 UTC,
/// so names sort by time. Only the newest `keep` files are kept, the current one included.
///
/// Blocking `std::fs`, because `tracing` writers are synchronous.
pub struct LogFiles {
    dir: PathBuf,
    keep: AtomicUsize,
    current_name: String,
    current: Mutex<File>,
}

pub struct FileInfo {
    pub name: String,
    pub started: Time,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
    pub current: bool,
}

impl LogFiles {
    /// Starts this run's file, then deletes the oldest others past `keep`.
    pub fn open(dir: &Path, keep: usize, started: Time) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let current_name = file_name(started);
        // A restart within the same second carries on in that file.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(&current_name))?;
        let keep = keep.max(1);
        let files = Self {
            dir: dir.to_owned(),
            keep: AtomicUsize::new(keep),
            current_name,
            current: Mutex::new(file),
        };
        files.prune(keep)?;
        Ok(files)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn keep(&self) -> usize {
        self.keep.load(Ordering::Relaxed)
    }

    /// Takes effect at once: the oldest files past the new count are deleted now.
    pub fn set_keep(&self, keep: usize) -> io::Result<()> {
        let keep = keep.max(1);
        let _current = self.lock();
        self.keep.store(keep, Ordering::Relaxed);
        self.prune(keep)
    }

    /// Newest first.
    pub fn list(&self) -> io::Result<Vec<FileInfo>> {
        let mut files = Vec::new();
        for (started, name) in self.runs()?.into_iter().rev() {
            let metadata = match fs::metadata(self.dir.join(&name)) {
                Ok(metadata) => metadata,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            files.push(FileInfo {
                current: name == self.current_name,
                name,
                started,
                bytes: metadata.len(),
                modified: metadata.modified().ok(),
            });
        }
        Ok(files)
    }

    /// One file, by a name from `list`. Anything that isn't a log file name is refused, so the
    /// name can't reach outside the log folder.
    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        if !is_log_name(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} isn't a log file"),
            ));
        }
        fs::read(self.dir.join(name))
    }

    /// Every file, oldest first.
    pub fn read_all(&self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        for (_, name) in self.runs()? {
            match fs::read(self.dir.join(name)) {
                Ok(bytes) => out.extend(bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    /// At most `max` bytes from the end, starting at a whole line.
    pub fn tail(&self, max: usize) -> io::Result<Vec<u8>> {
        let all = self.read_all()?;
        Ok(tail_lines(&all, max).to_vec())
    }

    /// Deletes the oldest files until `keep` are left. The current one is never deleted, even if
    /// the clock went back and it isn't the newest.
    fn prune(&self, keep: usize) -> io::Result<()> {
        let others: Vec<_> = self
            .runs()?
            .into_iter()
            .filter(|(_, name)| *name != self.current_name)
            .collect();
        let extra = others.len().saturating_sub(keep - 1);
        for (_, name) in &others[..extra] {
            remove_if_exists(&self.dir.join(name))?;
        }
        Ok(())
    }

    /// The log files in the folder, oldest first. Other files are ignored.
    fn runs(&self) -> io::Result<Vec<(Time, String)>> {
        let mut runs = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let Ok(name) = entry?.file_name().into_string() else {
                continue;
            };
            if let Some(started) = parse_name(&name) {
                runs.push((started, name));
            }
        }
        runs.sort();
        Ok(runs)
    }

    fn lock(&self) -> MutexGuard<'_, File> {
        // A panic while writing a line leaves nothing half-updated worth refusing over.
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// `tracing_subscriber` writes one event per `write` call through `Arc<LogFiles>`.
impl Write for &LogFiles {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Never log from here: it would come back to this writer.
        if let Err(e) = self.lock().write_all(buf) {
            eprintln!("couldn't write to the log file: {e}");
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn file_name(started: Time) -> String {
    format!("{PREFIX}{started}{SUFFIX}")
}

fn parse_name(name: &str) -> Option<Time> {
    name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.parse().ok()
}

pub fn is_log_name(name: &str) -> bool {
    parse_name(name).is_some() && !name.contains(['/', '\\'])
}

fn tail_lines(bytes: &[u8], max: usize) -> &[u8] {
    if bytes.len() <= max {
        return bytes;
    }
    let start = bytes.len() - max;
    match bytes[start..].iter().position(|&b| b == b'\n') {
        Some(newline) => &bytes[start + newline + 1..],
        None => &bytes[start..],
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn at(seconds: u64) -> Time {
        Time::from(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    /// Opens a run started at `seconds` and writes `lines` to it.
    fn run(dir: &Path, keep: usize, seconds: u64, lines: &[&str]) -> LogFiles {
        let files = LogFiles::open(dir, keep, at(seconds)).unwrap();
        for line in lines {
            (&files).write_all(format!("{line}\n").as_bytes()).unwrap();
        }
        files
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn names_files_by_start_time() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 3, 60, &["aaa"]);
        assert_eq!(names(dir.path()), ["shelf-1970-01-01T00:01:00Z.log"]);
    }

    #[test]
    fn each_start_begins_a_new_file_and_keeps_the_newest_n() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 2, 1, &["aaa"]);
        run(dir.path(), 2, 2, &["bbb"]);
        let files = run(dir.path(), 2, 3, &["ccc"]);
        assert_eq!(
            names(dir.path()),
            [
                "shelf-1970-01-01T00:00:02Z.log",
                "shelf-1970-01-01T00:00:03Z.log"
            ]
        );
        assert_eq!(files.read_all().unwrap(), b"bbb\nccc\n");
    }

    #[test]
    fn a_restart_in_the_same_second_appends() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 3, 1, &["aaa"]);
        let files = run(dir.path(), 3, 1, &["bbb"]);
        assert_eq!(names(dir.path()).len(), 1);
        assert_eq!(files.read_all().unwrap(), b"aaa\nbbb\n");
    }

    #[test]
    fn lowering_keep_deletes_the_oldest_at_once() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 5, 1, &["aaa"]);
        run(dir.path(), 5, 2, &["bbb"]);
        let files = run(dir.path(), 5, 3, &["ccc"]);
        files.set_keep(2).unwrap();
        assert_eq!(files.read_all().unwrap(), b"bbb\nccc\n");
        files.set_keep(1).unwrap();
        assert_eq!(files.read_all().unwrap(), b"ccc\n");
    }

    #[test]
    fn never_deletes_the_current_file() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 5, 10, &["aaa"]);
        run(dir.path(), 5, 20, &["bbb"]);
        // The clock went back: this run is older than both.
        let files = run(dir.path(), 2, 5, &["ccc"]);
        assert_eq!(files.read_all().unwrap(), b"ccc\nbbb\n");
        let current = files.list().unwrap().into_iter().find(|f| f.current).unwrap();
        assert_eq!(current.started, at(5));
    }

    #[test]
    fn ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("shelf.log"), "old\n").unwrap();
        let files = run(dir.path(), 1, 1, &["aaa"]);
        assert!(names(dir.path()).contains(&"shelf.log".to_owned()));
        assert_eq!(files.read_all().unwrap(), b"aaa\n");
    }

    #[test]
    fn lists_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), 3, 1, &["aaa", "bbb"]);
        let files = run(dir.path(), 3, 2, &["ccc"]);
        let list = files.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].current);
        assert_eq!(list[0].started, at(2));
        assert_eq!(list[0].bytes, 4);
        assert!(!list[1].current);
        assert_eq!(list[1].bytes, 8);
    }

    #[test]
    fn reads_one_file_by_name_only() {
        let dir = tempfile::tempdir().unwrap();
        let files = run(dir.path(), 3, 1, &["aaa"]);
        let name = files.list().unwrap()[0].name.clone();
        assert_eq!(files.read(&name).unwrap(), b"aaa\n");
        assert!(files.read("../app_data.json").is_err());
        assert!(files.read("shelf.log").is_err());
    }

    #[test]
    fn tail_starts_at_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let files = run(dir.path(), 3, 1, &["aaa", "bbb", "ccc"]);
        assert_eq!(files.tail(6).unwrap(), b"ccc\n");
        assert_eq!(files.tail(100).unwrap(), b"aaa\nbbb\nccc\n");
    }
}
