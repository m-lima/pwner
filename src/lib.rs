#![deny(warnings, clippy::pedantic)]
#![warn(rust_2018_idioms)]

pub use process::*;

pub mod process;
#[cfg(feature = "async")]
pub mod tokio;

/// A process builder, providing the wrapped handle, as well as piped handles to stdin,
/// stdout, and stderr.
///
/// The handle also implements a clean shutdown of the process upon destruction.
pub trait PipedSpawner<ReadSource, Stdin, Stdout, Stderr> {
    type Output: Process<ReadSource, Stdin, Stdout, Stderr>;

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
    /// use pwner::PipedSpawner;
    ///
    /// Command::new("ls")
    ///         .spawn_piped()
    ///         .expect("ls command failed to start");
    /// ```
    ///
    /// # Errors
    ///
    /// * If `command` fails to spawn
    fn spawn_piped(&mut self) -> std::io::Result<Self::Output>;
}

pub trait Process<ReadSource, Stdin, Stdout, Stderr>: std::ops::Drop {
    /// Returns the OS-assigned process identifier associated with this child.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::process::Command;
    /// use pwner::PipedSpawner;
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_piped() {
    ///     println!("Child's ID is {}", child.id());
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    fn id(&self) -> u32;

    /// Returns the OS-assigned process identifier associated with this child as a `pid_t`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::process::Command;
    /// use pwner::{PipedSpawner, Process};
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_piped() {
    ///     println!("Child's PID is {}", child.pid());
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[cfg(unix)]
    #[must_use]
    fn pid(&self) -> nix::unistd::Pid;

    /// Choose which pipe to read form next.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::Read;
    /// use std::process::Command;
    /// use pwner::{PipedSpawner, Process};
    /// use pwner::process::ReadSource;
    ///
    /// let mut child = Command::new("ls").spawn_piped().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// child.read_from(ReadSource::Stdout).read(&mut buffer).unwrap();
    /// ```
    fn read_from(&mut self, read_source: ReadSource) -> &mut Self;

    /// Decomposes the handle into mutable references to the pipes.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::Read;
    /// use std::process::{ Command, ChildStdin, ChildStdout };
    /// use pwner::{PipedSpawner, Process};
    /// use pwner::process::ReadSource;
    ///
    /// let mut child = Command::new("cat").spawn_piped().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdin, stdout, _) = child.decompose();
    ///
    /// stdin.write_all(b"hello\n").unwrap();
    /// stdout.read(&mut buffer).unwrap();
    /// ```
    fn decompose(&mut self) -> (&mut Stdin, &mut Stdout, &mut Stderr);
}

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
