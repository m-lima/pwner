# pwner
[![Github](https://github.com/m-lima/pwner/workflows/build/badge.svg)](https://github.com/m-lima/pwner/actions?workflow=build)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Cargo](https://img.shields.io/crates/v/pwner.svg)](https://crates.io/crates/pwner)
[![Documentation](https://docs.rs/pwner/badge.svg)](https://docs.rs/pwner)

Pwner is a Process Owner crate that allows ergonomic access to child processes.

This module creates the possibility of owning a child and having convenient methods to read and write, while also killing the process gracefully upon dropping.


## Spawning an owned process


```rust
use std::process::Command;
use pwner::Spawner;

Command::new("ls").spawn_owned().expect("ls command failed to start");
```


## Reading and writing


```rust
use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use pwner::Spawner;

let mut child = Command::new("cat").spawn_owned()?;
child.write_all(b"hello\n")?;

let mut output = String::new();
let mut reader = BufReader::new(child);
reader.read_line(&mut output)?;

assert_eq!("hello\n", output);
```


## Stopping an owned process

The owned process is terminated whenever it is dropped.


### Example


```rust
use std::process::Command;
use pwner::Spawner;

{
    let child = Command::new("ls").spawn_owned().expect("ls command failed to start");
}
// child is killed when dropped out of scope
```


## Graceful dropping

**Note:** Only available on *nix platforms.

When the owned process gets dropped, [`Process`][__link0] will try to kill it gracefully by sending a `SIGINT`. If the process still doesn’t die, a `SIGTERM` is sent and another chance is given, until finally a `SIGKILL` is sent.


 [__link0]: https://docs.rs/pwner/0.1.8/pwner/trait.Process.html
