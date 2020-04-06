#![deny(warnings, clippy::pedantic)]
#![warn(rust_2018_idioms)]

pub mod process;
pub use process::*;

#[cfg(feature = "async")]
pub mod async_process;

pub trait PipedSpawner {
    type Output;

    /// Spawn a new child for the given command
    ///
    /// # Arguments
    ///
    /// * `command` - The command to spawn as a child process
    ///
    /// # Errors
    ///
    /// * If `command` fails to spawn
    fn spawn_piped(&mut self) -> std::io::Result<Self::Output>;
}

#[cfg(unix)]
enum UnixIoError {
    Io(std::io::Error),
    Unix(nix::Error),
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
