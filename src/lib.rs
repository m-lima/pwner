#![deny(warnings, clippy::pedantic)]
#![warn(rust_2018_idioms)]

pub mod process;
#[cfg(feature = "async")]
pub mod tokio;

/// A process builder, providing the wrapped handle, as well as piped handles to stdin,
/// stdout, and stderr.
///
/// The handle also implements a clean shutdown of the process upon destruction.
pub trait PipedSpawner {
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
    /// use pwner::PipedSpawner;
    ///
    /// Command::new("ls")
    ///         .spawn_piped()
    ///         .expect("ls command failed to start");
    /// ```
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] if failure when spawning
    ///
    /// [`std::io::Error`]: std::io::Error
    fn spawn_piped(&mut self) -> std::io::Result<Self::Output>;
}

pub trait Process: std::ops::Drop {
    /// Returns the OS-assigned process identifier associated with this child.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::process::Command;
    /// use pwner::{ PipedSpawner, Process };
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
