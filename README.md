procmaps.rs
===========

[![Build Status](https://img.shields.io/github/workflow/status/woodruffw/procmaps.rs/CI/master)](https://github.com/woodruffw/procmaps.rs/actions?query=workflow%3ACI)

*procmaps.rs* is a (very) small Rust library with one job: parsing the memory
maps that Linux (and some other Unices) expose via `/proc/<pid>/maps` with
a pleasant structure.

I wrote it before realizing that [procmaps](https://github.com/jabedude/procmaps) already exists.
You should probably use that one instead, since it's nearly identical.

A quick sample:

```rust
let maps = rsprocmaps::from_pid(9001)?;

for map in maps {
  println!("{} executable? {}", map.address_range, map.permissions.executable);
}
```

Full documentation is available on [docs.rs](https://docs.rs/crate/rsprocmaps).

## Goals

* Parsing `/proc/<pid>/maps` correctly and into a clean structure

## Anti-goals

* Parsing other parts of `/proc`
* Resolving fundamental ambiguities in the `maps` file format (e.g. newlines and deleted pathnames)
