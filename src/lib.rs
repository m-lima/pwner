#![deny(warnings, clippy::pedantic)]
#![warn(rust_2018_idioms)]

pub mod process;

pub use process::*;

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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
