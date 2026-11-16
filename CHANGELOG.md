# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-11-15

### Added

- Clean up `$PATH`: drop empty, relative, missing and non-directory entries.
- Remove duplicates after resolving symlinks and trailing slashes; the first
  occurrence wins.
- Input from `$PATH` (default), `--env NAME`, `--value LIST`, or stdin (`-`).
- `--fish` output (one entry per line) for `set -gx PATH (pathscrub --fish)`.
- `--canonical`, `--keep-missing`, `--check` and `-v/--verbose`.
- Hint for directories whose names contain `:`.

[Unreleased]: https://github.com/appe233/pathscrub/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/appe233/pathscrub/releases/tag/v0.1.0
