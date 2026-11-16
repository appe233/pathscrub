# pathscrub

[![CI](https://github.com/appe233/pathscrub/actions/workflows/ci.yml/badge.svg)](https://github.com/appe233/pathscrub/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pathscrub.svg)](https://crates.io/crates/pathscrub)

A tiny, dependency-free tool that cleans up `$PATH` (or any colon-separated
path list) for Unix-like systems.

It reads the list, probes every entry, and prints a cleaned-up value with:

- **empty entries** removed (`::`, leading/trailing `:` — these silently mean
  "current directory", a security risk)
- **relative entries** removed (`bin`, `.`, unexpanded `~/bin`, `$HOME/bin`)
- **missing entries and non-directories** removed
- **duplicates** removed — compared after resolving symlinks, so
  `/usr/bin`, `/usr/bin/` and a symlink to `/usr/bin` are all the same.
  The **first** occurrence wins, so lookup order is preserved.

## Install

From [crates.io](https://crates.io/crates/pathscrub):

```sh
cargo install pathscrub
```

Prebuilt binaries for Linux (static, musl) and macOS, x86_64 and arm64, are
on the [releases page](https://github.com/appe233/pathscrub/releases):

```sh
tar xzf pathscrub-v*-<target>.tar.gz
cp pathscrub-v*-<target>/pathscrub ~/.local/bin/
```

From source:

```sh
cargo install --path .
```

Requires Rust 1.70 or newer.

## Usage

```sh
# POSIX shells (sh, bash, zsh) — e.g. at the end of ~/.zshrc
export PATH="$(pathscrub)"

# fish — e.g. at the end of ~/.config/fish/config.fish
set -gx PATH (pathscrub --fish)
```

```
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
```

The cleaned value goes to **stdout**; reports go to **stderr**, so `-v` is safe
to combine with `$(...)`.

### Exit codes

| code | meaning                                          |
|------|--------------------------------------------------|
| 0    | success (with `--check`: nothing to remove)      |
| 1    | `--check` only: something would be removed      |
| 2    | usage error, unset variable, I/O error           |

## Example

A real macOS `$PATH`:

```console
$ pathscrub --check
removed "/Users/appe/.pi/agent/bin": not found
removed "/Users/appe/.local/bin": duplicate (resolves to /Users/appe/.local/bin)
removed "/var/run/com.apple.security.cryptexd/codex.system/bootstrap/usr/local/bin": not found
removed "/var/run/com.apple.security.cryptexd/codex.system/bootstrap/usr/bin": not found
removed "/var/run/com.apple.security.cryptexd/codex.system/bootstrap/usr/appleinternal/bin": not found
$ echo $?
1
```

Other inputs:

```console
$ pathscrub -v --value '/usr/bin::/usr/bin/:bin:~/x:/nope:/etc/hosts'
removed "": empty entry
removed "/usr/bin/": duplicate (resolves to /usr/bin)
removed "bin": relative path
removed "~/x": relative path (unexpanded ~ or $VAR?)
removed "/nope": not found
removed "/etc/hosts": not a directory
/usr/bin

$ echo '/bin:/bin' | pathscrub - --fish
/bin

$ pathscrub --env MANPATH
```

## Details

- **Output spelling.** By default each kept entry is printed as you wrote it
  (minus trailing slashes), so deliberate symlinks are kept intact.
  `--canonical` prints the resolved path instead.
- **`--keep-missing`.** Keeps absolute entries that don't exist yet (e.g. a
  toolchain not installed yet, an unmounted volume). These are de-duplicated
  by their text.
- **Directories containing `:`.** PATH cannot represent them — every program
  splits `/opt/my:tools` into `/opt/my` and `tools`. pathscrub drops the pieces as
  usual, and with `-v`/`--check` tries rejoining neighbouring pieces; if that
  forms a real directory it prints a hint:

  ```
  hint: /opt/my:tools is a directory whose name contains ':'; PATH cannot represent it (rename it or use a symlink)
  ```

- **`--fish` and newlines.** An entry containing a newline cannot be printed
  one-per-line, so it is dropped in `--fish` mode.
- **Non-UTF-8 paths** are handled byte-exactly.
- **macOS cryptex paths.** `/var/run/com.apple.security.cryptexd/...` entries
  are added by `path_helper` and often don't exist; pathscrub drops them like any
  other missing directory. Use `--keep-missing` if you want to keep them.
- **Not a shell.** `~` and `$VAR` are not expanded; your shell config should
  write absolute paths into `$PATH`.

## License

[MIT](LICENSE)
