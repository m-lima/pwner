#![allow(clippy::needless_doctest_main)]

//! This module holds the tokio implementation of an async process.
//!
//! All potentially blocking interactions are performed async, including the dropping
//! of child processes (on *nix platforms).
//!
//! # Spawning an owned tokio process
//!
//! ```no_run
//! use tokio::process::Command;
//! use pwner::Spawner;
//!
//! Command::new("ls").spawn_owned().expect("ls command failed to start");
//! ```
//!
//! # Reading from the process
//!
//! ```no_run
//! # async fn wrapper() -> Result<(), Box<dyn std::error::Error>> {
//! use tokio::io::AsyncReadExt;
//! use tokio::process::Command;
//! use pwner::Spawner;
//!
//! let mut child = Command::new("ls").spawn_owned()?;
//! let mut output = String::new();
//! child.read_to_string(&mut output).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Writing to the process
//!
//! ```no_run
//! # async fn wrapper() -> Result<(), Box<dyn std::error::Error>> {
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//! use tokio::process::Command;
//! use pwner::Spawner;
//!
//! let mut child = Command::new("cat").spawn_owned()?;
//! child.write_all(b"hello\n").await?;
//!
//! let mut buffer = [0_u8; 10];
//! child.read(&mut buffer).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Graceful dropping
//!
//! **Note:** Only available on *nix platforms.
//!
//! When the owned process gets dropped, [`Process`](trait.Process.html) will try to
//! kill it gracefully by sending a `SIGINT` and asynchronously wait for the process to die for 2
//! seconds. If the process still doesn't die, a `SIGTERM` is sent and another chance is given,
//! until finally a `SIGKILL` is sent.
//!
//! ## Panics
//!
//! If the process is dropped without a tokio runtime, a panic will occur.
//!
//! ```should_panic
//! use tokio::process::Command;
//! use pwner::Spawner;
//!
//! {
//!     let child = Command::new("ls").spawn_owned().expect("ls command failed to start");
//! }
//! ```
//!
//! Make sure that a runtime is available to kill the child process
//!
//! ```
//! use tokio::process::Command;
//! use pwner::Spawner;
//!
//! #[tokio::main]
//! async fn main() {
//!     let child = Command::new("ls").spawn_owned().expect("ls command failed to start");
//! }
//! ```

/// Possible sources to read from
#[derive(Debug, Copy, Clone)]
pub enum ReadSource {
    /// Read from the child's stdout
    Stdout,
    /// Read from the child's stderr
    Stderr,
    /// Read from whichever has data available, the child's stdout or stderr
    Both,
}

/// An implementation of [`Process`](../trait.Process.html) that uses
/// [`tokio::process`](tokio::process) as the launcher.
///
/// All read and write operations are async.
///
/// **Note:** On *nix platforms, the owned process will have 2 seconds between signals, which is
/// run in a spawned task.
///
/// # Panics
///
/// When, on *nix platforms, a process gets dropped without a runtime.
pub struct Process(Option<ProcessImpl>, ReadSource);

impl crate::Spawner for tokio::process::Command {
    type Output = Process;

    fn spawn_owned(&mut self) -> std::io::Result<Self::Output> {
        let mut process = self
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();

        Ok(Process(
            Some(ProcessImpl {
                process,
                stdin,
                stdout,
                stderr,
            }),
            ReadSource::Both,
        ))
    }
}

impl super::Process for Process {
    /// Returns the OS-assigned process identifier associated with this child.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    /// use pwner::{ Spawner, Process };
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_owned() {
    ///     println!("Child's ID is {}", child.id());
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    fn id(&self) -> u32 {
        self.0.as_ref().unwrap().process.id()
    }
}

impl Process {
    /// Choose which pipe to read form next.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use tokio::io::AsyncReadExt;
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    /// use pwner::tokio::ReadSource;
    ///
    /// let mut child = Command::new("ls").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    ///
    /// child.read_from(ReadSource::Both).read(&mut buffer).await.unwrap();
    /// # };
    /// ```
    pub fn read_from(&mut self, read_source: ReadSource) -> &mut Self {
        self.1 = read_source;
        self
    }

    /// Decomposes the handle into mutable references to the pipes.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use tokio::io::{ AsyncReadExt, AsyncWriteExt };
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut child = Command::new("cat").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdin, stdout, _) = child.decompose();
    ///
    /// stdin.write_all(b"hello\n").await.unwrap();
    /// stdout.read(&mut buffer).await.unwrap();
    /// # };
    /// ```
    pub fn decompose(
        &mut self,
    ) -> (
        &mut tokio::process::ChildStdin,
        &mut tokio::process::ChildStdout,
        &mut tokio::process::ChildStderr,
    ) {
        let handle = self.0.as_mut().unwrap();
        (&mut handle.stdin, &mut handle.stdout, &mut handle.stderr)
    }
}

impl std::ops::Drop for Process {
    fn drop(&mut self) {
        if self.0.is_some() {
            tokio::spawn(self.0.take().unwrap().shutdown());
        }
    }
}

impl tokio::io::AsyncWrite for Process {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_shutdown(cx)
    }
}

impl tokio::io::AsyncRead for Process {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.1 {
            ReadSource::Stdout => {
                std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdout).poll_read(cx, buf)
            }
            ReadSource::Stderr => {
                std::pin::Pin::new(&mut self.0.as_mut().unwrap().stderr).poll_read(cx, buf)
            }
            ReadSource::Both => {
                let stderr =
                    std::pin::Pin::new(&mut self.0.as_mut().unwrap().stderr).poll_read(cx, buf);
                if stderr.is_ready() {
                    stderr
                } else {
                    std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdout).poll_read(cx, buf)
                }
            }
        }
    }
}

struct ProcessImpl {
    process: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
}

impl ProcessImpl {
    // Allowed because we are already assuming *nix
    #[allow(clippy::cast_possible_wrap)]
    #[cfg(unix)]
    pub fn pid(&self) -> nix::unistd::Pid {
        nix::unistd::Pid::from_raw(self.process.id() as nix::libc::pid_t)
    }

    #[cfg(not(unix))]
    async fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        self.process.kill();
        self.process.await
    }

    #[cfg(unix)]
    async fn shutdown(mut self) -> Result<std::process::ExitStatus, crate::UnixIoError> {
        use tokio::io::AsyncWriteExt;

        // Copy pid
        let pid = self.pid();

        // Close stdin
        self.stdin.flush().await?;
        std::mem::drop(self.stdin);

        // Close outputs
        std::mem::drop(self.stdout);
        std::mem::drop(self.stderr);

        // Pin the process
        let mut process = self.process;
        let mut process = std::pin::Pin::new(&mut process);

        {
            use nix::sys::signal;
            use std::time::Duration;
            use tokio::time::timeout;

            if timeout(Duration::from_secs(2), &mut process).await.is_err() {
                // Try SIGINT
                signal::kill(pid, signal::SIGINT)?;
            }

            if timeout(Duration::from_secs(2), &mut process).await.is_err() {
                // Try SIGTERM
                signal::kill(pid, signal::SIGTERM)?;
            }

            if timeout(Duration::from_secs(2), &mut process).await.is_err() {
                // Go for the kill
                process.kill()?;
            }
        }

        // Block until process is freed
        process.await.map_err(crate::UnixIoError::from)
    }
}

#[cfg(all(test, unix))]
mod test {
    use crate::Spawner;

    #[tokio::test]
    async fn test_read() {
        use tokio::io::AsyncReadExt;

        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("echo hello")
            .spawn_owned()
            .unwrap();
        let mut output = String::new();
        assert!(child.read_to_string(&mut output).await.is_ok());

        assert_eq!("hello\n", output);
    }

    #[tokio::test]
    async fn test_write() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut child = tokio::process::Command::new("cat").spawn_owned().unwrap();
        assert!(child.write_all(b"hello\n").await.is_ok());

        let mut buffer = [0_u8; 10];
        let bytes = child.read(&mut buffer).await.unwrap();
        assert_eq!("hello\n", std::str::from_utf8(&buffer[..bytes]).unwrap());
    }

    #[tokio::test]
    async fn test_drop_does_not_panic() {
        let mut child = tokio::process::Command::new("ls").spawn_owned().unwrap();
        assert!(child.0.take().unwrap().shutdown().await.is_ok());
    }
}
