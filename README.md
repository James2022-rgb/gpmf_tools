<div align="center">

# `gpmf_tools`📹🦀

**CLI tool and core libraries written in 🦀Rust for handling GPS streams in MP4 files recorded with GoPro action cameras**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

# 📦 The CLI tool binary

## Command line options

### Logging verbosity
`--verbose` / `-v` specifies verbose logging.
Note that logging is output to stderr, not stdio.

### Subcommand `extract-gpx`
Extracts the GPS stream (`gpmd`) from a GoPro MP4 file and exports it as a GPX file.

#### Input 
- Accepts a file path via `--input` / `-i`. Mandatory.

#### Output
- Writes to a file path specified via `--output` / `-o`, or to stdout if `--stdout` is set.
- If `--output` is not provided, the program writes to stdout only if `--stdout` is explicitly set.

### The help `-h, --help` option
The output of `gpmf_tools help` is quoted verbatim here:
```bash
Usage: gpmf_tools.exe [OPTIONS] <COMMAND>

Commands:
  extract-gpx  Extracts GPS data from a GoPro MP4 file and saves it as a GPX file.
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Verbose logging.
  -h, --help     Print help
  -V, --version  Print version
```
