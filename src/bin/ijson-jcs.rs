// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// I-JSON/JCS file operations. No shell, network, or implicit format migration.

#![forbid(unsafe_code)]

use ijson_jcs::{JsonMode, parse_json_bytes, to_jcs};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

const EXIT_OK: u8 = 0;
const EXIT_NOT_CANONICAL: u8 = 1;
const EXIT_INVALID: u8 = 2;
const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;

const USAGE: &str = "\
ijson-jcs - I-JSON (RFC 7493) parsing and JCS (RFC 8785) canonicalisation

USAGE:
    ijson-jcs check FILE...    Report whether each FILE is canonical
    ijson-jcs fix FILE...      Atomically replace valid regular files
    ijson-jcs canon            Canonicalise stdin to stdout
    ijson-jcs --help           Show this help
    ijson-jcs --version        Show the version

OPTIONS (before --):
    --max-bytes N              Input limit per file / stdin (default 67108864)
    --                        Treat all remaining arguments as file names

Input is parsed strictly as I-JSON: duplicate object keys, lone surrogates,
noncharacters, invalid UTF-8, numbers outside the IEEE-754 double range and
integers outside +/-(2^53-1) are rejected.

check accepts the JCS form optionally followed by ONE newline (\\n).
fix writes one final newline; canon writes none. The estate file profile
requires that final newline; check alone is not the complete estate gate.
fix refuses symlinks, hard-linked files (Unix), read-only files and inputs
whose JCS output would be rejected by the strict integer-token profile.

EXIT STATUS:
    0  all inputs valid and canonical (check), or all fixed (fix, canon)
    1  check: some input valid but not canonical (and none invalid)
    2  invalid/unreadable input, resource limit, write failure, or usage error
";

/// Parse strictly and render JCS bytes, including the legacy error path.
fn canonical_form(input: &[u8]) -> Result<Vec<u8>, String> {
    let value = parse_json_bytes(input, JsonMode::Canonical).map_err(|e| {
        match e.as_validation_error().map(|v| v.path()) {
            Some(path) if !path.is_empty() => format!("{e} (at {path})"),
            _ => e.to_string(),
        }
    })?;
    to_jcs(&value).map_err(|e| e.to_string())
}

/// Read at most one byte beyond the limit, including from an unbounded pipe.
fn read_bounded(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other(format!(
            "input exceeds --max-bytes {limit}"
        )));
    }
    Ok(bytes)
}

/// Non-regular files (e.g. FIFOs) must not make a file check hang indefinitely.
fn read_file(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    read_bounded(File::open(path)?, limit)
}

/// Remove an uncommitted temporary file on every ordinary failure path.
struct PendingFile(PathBuf);

impl Drop for PendingFile {
    fn drop(&mut self) {
        // After a successful rename this path no longer exists. Cleanup is
        // best-effort: it must not hide the primary write/rename error.
        let _ = fs::remove_file(&self.0);
    }
}

/// Resolve an explicit or implicit parent without falling back across directories.
fn replacement_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Replace in the same directory only after a complete, synced write.
/// This is not a concurrent-writer lock; only permission bits are copied.
fn atomic_replace(path: &Path, bytes: &[u8], permissions: fs::Permissions) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    atomic_replace_with(path, bytes, permissions, |parent| {
        parent.join(format!(
            ".ijson-jcs.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

// The name source is injectable so collision and post-write failures can be
// tested deterministically, without races, permission assumptions or stubs.
fn atomic_replace_with(
    path: &Path,
    bytes: &[u8],
    permissions: fs::Permissions,
    mut temporary: impl FnMut(&Path) -> PathBuf,
) -> io::Result<()> {
    let parent = replacement_parent(path);
    for _ in 0..128 {
        let temp = temporary(parent);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temp) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let pending = PendingFile(temp);
        file.write_all(bytes)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending.0, path)?;
        return Ok(());
    }
    Err(io::Error::other("could not create a unique temporary file"))
}

/// Check the mutation preconditions before opening or replacing the target.
fn fix_permissions(path: &Path) -> io::Result<fs::Permissions> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::other(
            "fix requires a regular file, not a symlink or device",
        ));
    }
    if metadata.permissions().readonly() {
        return Err(io::Error::other("fix refuses a read-only file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(io::Error::other("fix refuses a hard-linked file"));
        }
    }
    Ok(metadata.permissions())
}

/// Process files independently; a bad file does not prevent checking the rest.
fn files_command(files: &[PathBuf], fix: bool, limit: u64, out: &mut impl Write) -> io::Result<u8> {
    let mut code = EXIT_OK;
    for path in files {
        let result = (|| -> Result<&str, String> {
            let permissions = if fix {
                Some(fix_permissions(path).map_err(|e| e.to_string())?)
            } else {
                None
            };
            let bytes = read_file(path, limit).map_err(|e| format!("cannot read: {e}"))?;
            let mut wanted = canonical_form(&bytes)?;
            if !fix {
                return Ok(if bytes.strip_suffix(b"\n").unwrap_or(&bytes) == wanted {
                    "OK"
                } else {
                    "NOT CANONICAL"
                });
            }
            // A finite float can render as a bare integer beyond 2^53-1.
            // Retain the existing strict token policy, but never replace a
            // valid input with a file that our own checker would reject.
            canonical_form(&wanted).map_err(|e| format!(
                "canonical output violates the strict file profile: {e}; use a string for exact integers, or canon for JCS bytes"
            ))?;
            wanted.push(b'\n');
            if wanted == bytes {
                return Ok("OK");
            }
            if let Some(permissions) = permissions {
                atomic_replace(path, &wanted, permissions)
                    .map_err(|e| format!("cannot replace: {e}"))?;
            }
            Ok("FIXED")
        })();
        match result {
            Ok(verdict) => {
                writeln!(out, "{verdict} {}", path.display())?;
                if verdict == "NOT CANONICAL" {
                    code = code.max(EXIT_NOT_CANONICAL);
                }
            }
            Err(reason) => {
                writeln!(out, "INVALID {}: {reason}", path.display())?;
                code = EXIT_INVALID;
            }
        }
    }
    Ok(code)
}

/// Print usage failures to stderr; never misclassify them as validation success.
fn usage_error(message: &str) -> u8 {
    eprintln!("ijson-jcs: {message}\nTry 'ijson-jcs --help' for usage.");
    EXIT_INVALID
}

/// Parse OS strings so non-UTF-8 file names work, respecting the `--` boundary.
fn run(args: &[OsString], out: &mut impl Write) -> io::Result<u8> {
    let mut positional = Vec::new();
    let mut options = true;
    let mut limit = DEFAULT_MAX_BYTES;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
        } else if options && (arg == "--help" || arg == "-h") {
            write!(out, "{USAGE}")?;
            return Ok(EXIT_OK);
        } else if options && (arg == "--version" || arg == "-V") {
            writeln!(out, "ijson-jcs {}", env!("CARGO_PKG_VERSION"))?;
            return Ok(EXIT_OK);
        } else if options && arg == "--max-bytes" {
            match args
                .next()
                .and_then(|v| v.to_str())
                .and_then(|v| v.parse::<u64>().ok())
            {
                Some(n) if n > 0 && n < usize::MAX as u64 => limit = n,
                _ => {
                    return Ok(usage_error(
                        "--max-bytes needs a positive, addressable byte count",
                    ));
                }
            }
        } else if options && arg.to_string_lossy().starts_with('-') && arg != "-" {
            return Ok(usage_error(&format!(
                "unknown option '{}'",
                arg.to_string_lossy()
            )));
        } else {
            positional.push(arg);
        }
    }
    let Some((command, files)) = positional.split_first() else {
        return Ok(usage_error("missing command"));
    };
    let files: Vec<PathBuf> = files.iter().map(|path| PathBuf::from(*path)).collect();
    match command.to_str() {
        Some("check" | "fix") if files.is_empty() => {
            Ok(usage_error("check/fix needs at least one FILE"))
        }
        Some("check" | "fix") => {
            files_command(&files, command.as_os_str() == OsStr::new("fix"), limit, out)
        }
        Some("canon") if !files.is_empty() => {
            Ok(usage_error("'canon' reads stdin and takes no FILE"))
        }
        Some("canon") => {
            let result = read_bounded(io::stdin().lock(), limit)
                .map_err(|e| format!("cannot read: {e}"))
                .and_then(|bytes| canonical_form(&bytes));
            match result {
                Ok(bytes) => {
                    out.write_all(&bytes)?;
                    Ok(EXIT_OK)
                }
                Err(reason) => {
                    eprintln!("INVALID <stdin>: {reason}");
                    Ok(EXIT_INVALID)
                }
            }
        }
        _ => Ok(usage_error(&format!(
            "unknown command '{}'",
            command.to_string_lossy()
        ))),
    }
}

/// Convert output errors (including broken pipes) into the documented exit 2.
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut out = io::stdout().lock();
    match run(&args, &mut out).and_then(|code| out.flush().map(|()| code)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ijson-jcs: cannot write stdout: {error}");
            ExitCode::from(EXIT_INVALID)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "ijson-jcs-atomic-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn permissions(&self) -> fs::Permissions {
            let file = self.0.join("permissions");
            fs::write(&file, b"fixture").unwrap();
            fs::metadata(file).unwrap().permissions()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bounded_reads_reject_valid_prefixes_with_oversized_suffixes() {
        assert_eq!(DEFAULT_MAX_BYTES, 67_108_864);
        assert_eq!(read_bounded(&b"{}"[..], 2).unwrap(), b"{}");
        for input in [&b"{} "[..], &b"{}\n"[..], &b"{}garbage"[..]] {
            let error = read_bounded(input, 2).unwrap_err();
            assert!(error.to_string().contains("exceeds --max-bytes 2"));
        }
        assert!(read_bounded(&b""[..], 0).unwrap().is_empty());
        assert!(read_bounded(&b" "[..], 0).is_err());
    }

    #[test]
    fn canonical_errors_keep_nested_paths_but_do_not_invent_root_paths() {
        let nested = canonical_form(br#"{"a/b":{"":9007199254740992}}"#).unwrap_err();
        assert!(nested.ends_with("(at a~1b/)"), "{nested}");
        let root = canonical_form(b"9007199254740992").unwrap_err();
        assert!(!root.contains("(at "), "{root}");
    }

    #[test]
    fn temporary_parent_is_always_the_replacement_directory() {
        assert_eq!(replacement_parent(Path::new("file")), Path::new("."));
        assert_eq!(replacement_parent(Path::new("")), Path::new("."));
        assert_eq!(replacement_parent(Path::new("dir/file")), Path::new("dir"));
        assert_eq!(
            replacement_parent(Path::new("/absolute/file")),
            Path::new("/absolute")
        );
    }

    #[test]
    fn exclusive_creation_retries_collisions_without_clobbering() {
        let work = Scratch::new();
        let occupied = work.0.join("occupied");
        let target = work.0.join("target");
        fs::write(&occupied, b"must survive").unwrap();
        let mut attempts = 0;
        atomic_replace_with(&target, b"new bytes", work.permissions(), |parent| {
            assert_eq!(parent, work.0);
            attempts += 1;
            parent.join(if attempts == 1 { "occupied" } else { "free" })
        })
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(fs::read(&occupied).unwrap(), b"must survive");
        assert_eq!(fs::read(&target).unwrap(), b"new bytes");
        assert!(!work.0.join("free").exists());

        let mut attempts = 0;
        let error = atomic_replace_with(&target, b"never written", work.permissions(), |_| {
            attempts += 1;
            occupied.clone()
        })
        .unwrap_err();
        assert_eq!(attempts, 128);
        assert!(error.to_string().contains("unique temporary file"));
        assert_eq!(fs::read(&target).unwrap(), b"new bytes");
    }

    #[test]
    fn non_collision_errors_are_not_retried_or_reclassified() {
        let work = Scratch::new();
        let target = work.0.join("missing-parent/target");
        let mut attempts = 0;
        let error = atomic_replace_with(&target, b"data", work.permissions(), |parent| {
            attempts += 1;
            parent.join("temp")
        })
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn failed_rename_cleans_written_temporary_file_and_preserves_destination() {
        let work = Scratch::new();
        let target = work.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("original"), b"keep").unwrap();
        let temp = work.0.join("temporary");
        assert!(
            atomic_replace_with(&target, b"new data", work.permissions(), |_| temp.clone())
                .is_err()
        );
        assert!(!temp.exists(), "failed replacement leaked its temporary file");
        assert_eq!(fs::read(target.join("original")).unwrap(), b"keep");
    }
}
