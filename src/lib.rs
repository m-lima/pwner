#![deny(warnings, missing_docs, clippy::pedantic, clippy::all)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

//! Pwner is a Process Owner crate that allows ergonomic access to child processes.
//!
//! This module creates the possibility of owning a child and having convenient methods to
//! read and write, while also killing the process gracefully upon dropping.
//!
//! # Spawning an owned process
//!
//! ```no_run
//! use std::process::Command;
//! use pwner::Spawner;
//!
//! Command::new("ls").spawn_owned().expect("ls command failed to start");
//! ```
//!
//! # Reading and writing
//!
//! ```no_run
//! use std::io::{BufRead, BufReader, Write};
//! use std::process::Command;
//! use pwner::Spawner;
//!
//! # fn wrapper() -> std::io::Result<()> {
//! let mut child = Command::new("cat").spawn_owned()?;
//! child.write_all(b"hello\n")?;
//!
//! let mut output = String::new();
//! let mut reader = BufReader::new(child);
//! reader.read_line(&mut output)?;
//!
//! assert_eq!("hello\n", output);
//! # Ok(())
//! # }
//! ```
//!
//! # Stopping an owned process
//!
//! The owned process is terminated whenever it is dropped.
//!
//! ## Example
//!
//! ```no_run
//! use std::process::Command;
//! use pwner::Spawner;
//!
//! {
//!     let child = Command::new("ls").spawn_owned().expect("ls command failed to start");
//! }
//! // child is killed when dropped out of scope
//! ```
//!
//! # Graceful dropping
//!
//! **Note:** Only available on *nix platforms.
//!
//! When the owned process gets dropped, [`Process`](trait.Process.html) will try to
//! kill it gracefully by sending a `SIGINT`. If the process still doesn't die,
//! a `SIGTERM` is sent and another chance is given, until finally a `SIGKILL` is sent.
pub mod process;
#[cfg(feature = "tokio")]
pub mod tokio;

/// A process builder, providing the wrapped handle, as well as piped handles to stdin,
/// stdout, and stderr.
///
/// The handle also implements a clean shutdown of the process upon destruction.
pub trait Spawner {
    /// The [`Process`](trait.Process.html) implementation output by
    /// [`spawn_owned()`](#tymethod.spawn_owned)
    type Output: Process;

    /// Executes the command as a child process, returning a handle to it.
    ///
    /// Upon creation, stid, stdout, and stderr are piped and kept by the handle.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// Command::new("ls")
    ///         .spawn_owned()
    ///         .expect("ls command failed to start");
    /// ```
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] if failure when spawning
    ///
    /// [`std::io::Error`]: std::io::Error
    fn spawn_owned(&mut self) -> std::io::Result<Self::Output>;
}

/// The trait returned by [`Spawner::spawn_owned()`](trait.Spawner.html#tymethod.spawn_owned).
///
/// All implementations of [`Spawner`](trait.Spawner.html) must return a concrete instance capable
/// of read/write.
pub trait Process {}

#[cfg(unix)]
#[derive(Debug)]
enum UnixIoError {
    Io(std::io::Error),
    Unix(nix::Error),
}

#[cfg(unix)]
impl std::error::Error for UnixIoError {}

#[cfg(unix)]
impl std::fmt::Display for UnixIoError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnixIoError::Io(e) => e.fmt(fmt),
            UnixIoError::Unix(e) => e.fmt(fmt),
        }
    }
}

#[cfg(unix)]
impl std::convert::From<std::io::Error> for UnixIoError {
    fn from(error: std::io::Error) -> Self {
        UnixIoError::Io(error)
    }
}

#[cfg(unix)]
impl std::convert::From<nix::Error> for UnixIoError {
    fn from(error: nix::Error) -> Self {
        UnixIoError::Unix(error)
    }
}
