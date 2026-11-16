//! pathscrub — clean up $PATH (or any colon-separated path list).
//!
//! Keeps entries that are absolute, existing directories; drops empty,
//! relative, missing and duplicate entries (duplicates detected after
//! resolving symlinks; first occurrence wins).

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: pathscrub [options] [-]

Input (pick at most one; default: $PATH):
  --env NAME       read the variable NAME (e.g. MANPATH)
  --value LIST     use LIST literally, e.g. /a:/b
  -                read LIST from stdin

Options:
  --fish           print one entry per line:  set -gx PATH (pathscrub --fish)
  --canonical      print symlink-resolved paths instead of the original spelling
  --keep-missing   keep absolute entries that don't exist (yet)
  --check          print nothing to stdout; report problems, exit 1 if any
  -v, --verbose    report removed entries and reasons on stderr
  -h, --help       show this help

POSIX shells:  export PATH=\"$(pathscrub)\"
";

enum Source {
    Env(OsString),
    Value(OsString),
    Stdin,
}

struct Opts {
    source: Source,
    fish: bool,
    canonical: bool,
    keep_missing: bool,
    check: bool,
    verbose: bool,
}

fn die(msg: &str) -> ExitCode {
    eprintln!("pathscrub: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}

fn parse_args() -> Result<Opts, String> {
    let mut o = Opts {
        source: Source::Env("PATH".into()),
        fish: false,
        canonical: false,
        keep_missing: false,
        check: false,
        verbose: false,
    };
    let mut have_source = false;
    let mut set_source = |o: &mut Opts, s: Source| -> Result<(), String> {
        if have_source {
            return Err("only one of --env, --value, - may be given".into());
        }
        have_source = true;
        o.source = s;
        Ok(())
    };

    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        match a.as_bytes() {
            b"--env" => {
                let v = args.next().ok_or("--env needs a NAME")?;
                set_source(&mut o, Source::Env(v))?;
            }
            b"--value" => {
                let v = args.next().ok_or("--value needs a LIST")?;
                set_source(&mut o, Source::Value(v))?;
            }
            b"-" => set_source(&mut o, Source::Stdin)?,
            b"--fish" => o.fish = true,
            b"--canonical" => o.canonical = true,
            b"--keep-missing" => o.keep_missing = true,
            b"--check" => o.check = true,
            b"-v" | b"--verbose" => o.verbose = true,
            b"-h" | b"--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {}", a.to_string_lossy())),
        }
    }
    Ok(o)
}

fn read_input(src: &Source) -> Result<Vec<u8>, String> {
    match src {
        Source::Env(name) => std::env::var_os(name)
            .map(OsString::into_vec)
            .ok_or_else(|| format!("variable {} is not set", name.to_string_lossy())),
        Source::Value(v) => Ok(v.as_bytes().to_vec()),
        Source::Stdin => {
            let mut buf = Vec::new();
            io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("reading stdin: {e}"))?;
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            Ok(buf)
        }
    }
}

fn p(b: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(b))
}

/// Strip trailing slashes, keeping "/" itself.
fn trim_slashes(mut b: &[u8]) -> &[u8] {
    while b.len() > 1 && b.ends_with(b"/") {
        b = &b[..b.len() - 1];
    }
    b
}

/// Detect directories whose names contain ':' and were therefore split
/// apart: if pieces i..=j joined by ':' form an existing directory, report it.
fn colon_hints(pieces: &[&[u8]]) -> Vec<String> {
    let mut hints = Vec::new();
    for i in 0..pieces.len() {
        if !pieces[i].starts_with(b"/") {
            continue;
        }
        let mut joined = pieces[i].to_vec();
        for piece in &pieces[i + 1..] {
            joined.push(b':');
            joined.extend_from_slice(piece);
            if p(&joined).is_dir() {
                hints.push(format!(
                    "hint: {} is a directory whose name contains ':'; \
                     PATH cannot represent it (rename it or use a symlink)",
                    p(&joined).display()
                ));
            }
        }
    }
    hints
}

fn main() -> ExitCode {
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => return die(&e),
    };
    let input = match read_input(&o.source) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("pathscrub: {e}");
            return ExitCode::from(2);
        }
    };

    let report = o.verbose || o.check;
    let pieces: Vec<&[u8]> = input.split(|&c| c == b':').collect();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut kept: Vec<Vec<u8>> = Vec::new();
    let mut removed = 0usize;

    let mut drop = |entry: &[u8], why: &str| {
        removed += 1;
        if report {
            eprintln!("removed {:?}: {why}", OsStr::from_bytes(entry));
        }
    };

    for &raw in &pieces {
        if raw.is_empty() {
            drop(raw, "empty entry");
            continue;
        }
        if !raw.starts_with(b"/") {
            let why = if raw.starts_with(b"~") || raw.contains(&b'$') {
                "relative path (unexpanded ~ or $VAR?)"
            } else {
                "relative path"
            };
            drop(raw, why);
            continue;
        }
        if o.fish && raw.contains(&b'\n') {
            drop(raw, "contains a newline (unrepresentable in --fish output)");
            continue;
        }

        let entry = trim_slashes(raw);
        let path = p(entry);
        let (key, out) = match std::fs::metadata(path) {
            Ok(m) if m.is_dir() => match std::fs::canonicalize(path) {
                Ok(c) => {
                    let out = if o.canonical {
                        c.as_os_str().as_bytes().to_vec()
                    } else {
                        entry.to_vec()
                    };
                    (c, out)
                }
                Err(e) => {
                    drop(raw, &format!("cannot resolve: {e}"));
                    continue;
                }
            },
            Ok(_) => {
                drop(raw, "not a directory");
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound && o.keep_missing => {
                (path.to_path_buf(), entry.to_vec())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                drop(raw, "not found");
                continue;
            }
            Err(e) => {
                drop(raw, &e.to_string());
                continue;
            }
        };

        if seen.contains(&key) {
            drop(raw, &format!("duplicate (resolves to {})", key.display()));
            continue;
        }
        seen.insert(key);
        kept.push(out);
    }

    if report {
        for h in colon_hints(&pieces) {
            eprintln!("{h}");
        }
    }

    if o.check {
        return if removed > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    let sep: &[u8] = if o.fish { b"\n" } else { b":" };
    let mut out = kept.join(sep);
    if !o.fish || !out.is_empty() {
        out.push(b'\n');
    }
    let mut stdout = io::stdout().lock();
    if stdout.write_all(&out).and_then(|_| stdout.flush()).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
