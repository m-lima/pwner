#![allow(clippy::needless_doctest_main)]

//! Holds the tokio implementation of an async process.
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
//! When the owned process gets dropped, [`Process`](crate::Process) will try to kill it gracefully
//! by sending a `SIGINT` and asynchronously wait for the process to die for 2 seconds. If the
//! process still doesn't die, a `SIGTERM` is sent and another chance is given, until finally a
//! `SIGKILL` is sent.
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
//! #[tokio::main(flavor = "current_thread")]
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

/// An implementation of [`Process`](crate::Process) that uses [`tokio::process`] as the launcher.
///
/// All read and write operations are async.
///
/// **Note:** On *nix platforms, the owned process will have 2 seconds between signals, which is
/// run in a spawned task.
///
/// # Panics
///
/// When, on *nix platforms, a process gets dropped without a runtime.
pub struct Duplex(Simplex, Output);

impl super::Spawner for tokio::process::Command {
    type Output = Duplex;

    fn spawn_owned(&mut self) -> std::io::Result<Self::Output> {
        let mut process = self
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();

        Ok(Duplex(
            Simplex(Some(ProcessImpl { process, stdin })),
            Output {
                read_source: ReadSource::Both,
                stdout,
                stderr,
            },
        ))
    }
}

impl super::Process for Duplex {}

impl Duplex {
    /// Returns the OS-assigned process identifier associated with this child.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_owned() {
    ///     match child.id() {
    ///       Some(pid) => println!("Child's ID is {}", pid),
    ///       None => println!("Child has already exited"),
    ///     }
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.0.id()
    }

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
        self.1.read_from(read_source);
        self
    }

    /// Waits for the child to exit completely, returning the status with which it exited, stdout,
    /// and stderr.
    ///
    /// The stdin handle to the child process, if any, will be closed before waiting. This helps
    /// avoid deadlock: it ensures that the child does not block waiting for input from the parent,
    /// while the parent waits for the child to exit.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use pwner::Spawner;
    /// use tokio::io::{AsyncReadExt, BufReader};
    /// use tokio::process::Command;
    ///
    /// let child = Command::new("ls").spawn_owned().unwrap();
    /// let (status, stdout, stderr) = child.wait().await.unwrap();
    ///
    /// let mut buffer = String::new();
    /// if status.success() {
    ///     let mut reader = BufReader::new(stdout);
    ///     reader.read_to_string(&mut buffer).await.unwrap();
    /// } else {
    ///     let mut reader = BufReader::new(stderr);
    ///     reader.read_to_string(&mut buffer).await.unwrap();
    /// }
    /// # };
    /// ```
    ///
    /// # Errors
    ///
    /// Relays the error from [`tokio::process::Child::wait()`]
    pub async fn wait(
        self,
    ) -> Result<
        (
            std::process::ExitStatus,
            tokio::process::ChildStdout,
            tokio::process::ChildStderr,
        ),
        std::io::Error,
    > {
        let (mut child, _, stdout, stderr) = self.eject();
        child.wait().await.map(|status| (status, stdout, stderr))
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
    /// let (stdin, stdout, _) = child.pipes();
    ///
    /// stdin.write_all(b"hello\n").await.unwrap();
    /// stdout.read(&mut buffer).await.unwrap();
    /// # };
    /// ```
    pub fn pipes(
        &mut self,
    ) -> (
        &mut tokio::process::ChildStdin,
        &mut tokio::process::ChildStdout,
        &mut tokio::process::ChildStderr,
    ) {
        (self.0.stdin(), &mut self.1.stdout, &mut self.1.stderr)
    }

    /// Separates the process and its input from the output pipes. Ownership is retained by a
    /// [`Simplex`] which still implements a graceful drop of the child process.
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
    /// let child = Command::new("cat").spawn_owned().unwrap();
    /// let (mut input_only_process, mut output) = child.decompose();
    ///
    /// // Spawn printing task
    /// tokio::spawn(async move {
    ///     let mut buffer = [0; 1024];
    ///     while let Ok(bytes) = output.read(&mut buffer).await {
    ///         if let Ok(string) = std::str::from_utf8(&buffer[..bytes]) {
    ///             print!("{}", string);
    ///         }
    ///     }
    /// });
    ///
    /// // Interact normally with the child process
    /// input_only_process.write_all(b"hello\n").await.unwrap();
    /// # };
    /// ```
    #[must_use]
    pub fn decompose(self) -> (Simplex, Output) {
        (self.0, self.1)
    }

    /// Completely releases the ownership of the child process. The raw underlying process and
    /// pipes are returned and no wrapping function is applicable any longer.
    ///
    /// **Note:** By ejecting the process, graceful drop will no longer be available.
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
    /// let (process, mut stdin, mut stdout, _) = child.eject();
    ///
    /// stdin.write_all(b"hello\n").await.unwrap();
    /// stdout.read(&mut buffer).await.unwrap();
    ///
    /// // Graceful drop will not be executed for `child` as the ejected variable leaves scope here
    /// # };
    /// ```
    #[must_use]
    pub fn eject(
        self,
    ) -> (
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
        tokio::process::ChildStderr,
    ) {
        let (process, stdin) = self.0.eject();
        let (stdout, stderr) = self.1.eject();
        (process, stdin, stdout, stderr)
    }

    /// Consumes the process to allow awaiting for shutdown.
    ///
    /// This method is essentially the same as a `drop`, however it return a `Future` which allows
    /// the parent to await the shutdown.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let child = Command::new("top").spawn_owned().unwrap();
    /// child.shutdown().await.unwrap();
    /// # };
    /// ```
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] if failure when killing the process.
    pub async fn shutdown(self) -> std::io::Result<std::process::ExitStatus> {
        self.0.shutdown().await
    }
}

impl tokio::io::AsyncWrite for Duplex {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0).poll_write(ctx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_flush(ctx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_shutdown(ctx)
    }
}

impl tokio::io::AsyncRead for Duplex {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.1).poll_read(ctx, buf)
    }
}

/// An implementation of [`Process`](crate::Process) that is stripped from any output
/// pipes.
///
/// All write operations are async.
///
/// # Examples
///
/// ```no_run
/// use tokio::process::Command;
/// use pwner::Spawner;
///
/// let mut command = Command::new("ls");
/// if let Ok(child) = command.spawn_owned() {
///     let (input_only_process, _) = child.decompose();
/// } else {
///     println!("ls command didn't start");
/// }
/// ```
///
/// **Note:** On *nix platforms, the owned process will have 2 seconds between signals, which is a
/// blocking wait.
#[allow(clippy::module_name_repetitions)]
pub struct Simplex(Option<ProcessImpl>);

impl super::Process for Simplex {}

impl Simplex {
    /// Returns the OS-assigned process identifier associated with this child.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_owned() {
    ///     let (process, _) = child.decompose();
    ///     match process.id() {
    ///       Some(pid) => println!("Child's ID is {}", pid),
    ///       None => println!("Child has already exited"),
    ///     }
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.0
            .as_ref()
            .unwrap_or_else(|| unreachable!())
            .process
            .id()
    }

    fn stdin(&mut self) -> &mut tokio::process::ChildStdin {
        &mut self.0.as_mut().unwrap().stdin
    }

    /// Waits for the child to exit completely, returning the status with which it exited.
    ///
    /// The stdin handle to the child process, if any, will be closed before waiting. This helps
    /// avoid deadlock: it ensures that the child does not block waiting for input from the parent,
    /// while the parent waits for the child to exit.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use pwner::{tokio::ReadSource, Spawner};
    /// use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
    /// use tokio::process::Command;
    ///
    /// let (mut child, mut output) = Command::new("cat").spawn_owned().unwrap().decompose();
    ///
    /// child.write_all(b"Hello\n").await.unwrap();
    /// let status = child.wait().await.unwrap();
    ///
    /// let mut buffer = String::new();
    /// if status.success() {
    ///     output.read_from(ReadSource::Stdout);
    /// } else {
    ///     output.read_from(ReadSource::Stderr);
    /// }
    /// let mut reader = BufReader::new(output);
    /// reader.read_to_string(&mut buffer).await.unwrap();
    /// # };
    /// ```
    ///
    /// # Errors
    ///
    /// Relays the error from [`tokio::process::Child::wait()`]
    pub async fn wait(self) -> Result<std::process::ExitStatus, std::io::Error> {
        let (mut child, _) = self.eject();
        child.wait().await
    }

    /// Completely releases the ownership of the child process. The raw underlying process and
    /// pipes are returned and no wrapping function is applicable any longer.
    ///
    /// **Note:** By ejecting the process, graceful drop will no longer be available.
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
    /// let (child, mut output) = Command::new("cat").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// let (process, mut stdin) = child.eject();
    ///
    /// stdin.write_all(b"hello\n").await.unwrap();
    /// output.read(&mut buffer).await.unwrap();
    ///
    /// // Graceful drop will not be executed for `child` as the ejected variable leaves scope here
    /// # };
    /// ```
    #[must_use]
    pub fn eject(mut self) -> (tokio::process::Child, tokio::process::ChildStdin) {
        let process = self.0.take().unwrap_or_else(|| unreachable!());
        (process.process, process.stdin)
    }

    /// Consumes the process to allow awaiting for shutdown.
    ///
    /// This method is essentially the same as a `drop`, however it return a `Future` which allows
    /// the parent to await the shutdown.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// # async {
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let (child, _) = Command::new("top").spawn_owned().unwrap().decompose();
    /// child.shutdown().await.unwrap();
    /// # };
    /// ```
    ///
    /// # Errors
    ///
    /// If failure when killing the process.
    pub async fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        match self
            .0
            .take()
            .unwrap_or_else(|| unreachable!())
            .shutdown()
            .await
        {
            Ok(status) => Ok(status),
            Err(super::UnixIoError::Io(err)) => Err(err),
            Err(super::UnixIoError::Unix(err)) => {
                Err(std::io::Error::from_raw_os_error(err as i32))
            }
        }
    }
}

impl std::ops::Drop for Simplex {
    fn drop(&mut self) {
        if self.0.is_some() {
            tokio::spawn(self.0.take().unwrap().shutdown());
        }
    }
}

impl tokio::io::AsyncWrite for Simplex {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_write(ctx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_flush(ctx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0.as_mut().unwrap().stdin).poll_shutdown(ctx)
    }
}

/// A readable handle for both the stdout and stderr of the child process.
pub struct Output {
    read_source: ReadSource,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
}

impl Output {
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
    /// let (process, mut output) = Command::new("ls").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// output.read_from(ReadSource::Stdout).read(&mut buffer).await.unwrap();
    /// # };
    /// ```
    pub fn read_from(&mut self, read_source: ReadSource) -> &mut Self {
        self.read_source = read_source;
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
    /// use tokio::io::AsyncReadExt;
    /// use tokio::process::Command;
    /// use pwner::Spawner;
    ///
    /// let (process, mut output) = Command::new("ls").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdout, stderr) = output.pipes();
    ///
    /// stdout.read(&mut buffer).await.unwrap();
    /// # };
    /// ```
    pub fn pipes(
        &mut self,
    ) -> (
        &mut tokio::process::ChildStdout,
        &mut tokio::process::ChildStderr,
    ) {
        (&mut self.stdout, &mut self.stderr)
    }

    /// Consumes this struct returning the containing pipes.
    #[must_use]
    pub fn eject(self) -> (tokio::process::ChildStdout, tokio::process::ChildStderr) {
        (self.stdout, self.stderr)
    }
}

impl tokio::io::AsyncRead for Output {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.read_source {
            ReadSource::Stdout => std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf),
            ReadSource::Stderr => std::pin::Pin::new(&mut self.stderr).poll_read(cx, buf),
            ReadSource::Both => {
                let stderr = std::pin::Pin::new(&mut self.stderr).poll_read(cx, buf);
                if stderr.is_ready() {
                    stderr
                } else {
                    std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf)
                }
            }
        }
    }
}

struct ProcessImpl {
    process: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
}

impl ProcessImpl {
    // Allowed because we are already assuming *nix
    #[allow(clippy::cast_possible_wrap)]
    #[cfg(unix)]
    pub fn pid(&self) -> Option<nix::unistd::Pid> {
        self.process
            .id()
            .map(|pid| nix::unistd::Pid::from_raw(pid as nix::libc::pid_t))
    }

    #[cfg(not(unix))]
    async fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        self.process.kill();
        self.process.await
    }

    #[cfg(unix)]
    async fn shutdown(mut self) -> Result<std::process::ExitStatus, super::UnixIoError> {
        // Copy the pid if the child has not exited yet
        let pid = match self.process.try_wait() {
            Ok(None) => self.pid().unwrap(),
            Ok(Some(status)) => return Ok(status),
            Err(err) => return Err(super::UnixIoError::from(err)),
        };

        // Pin the process
        let mut process = self.process;
        let mut process = std::pin::Pin::new(&mut process);

        {
            use nix::sys::signal;
            use std::time::Duration;
            use tokio::time::timeout;

            if timeout(Duration::from_secs(2), process.wait())
                .await
                .is_err()
            {
                // Try SIGINT
                signal::kill(pid, signal::SIGINT)?;
            }

            if timeout(Duration::from_secs(2), process.wait())
                .await
                .is_err()
            {
                // Try SIGTERM
                signal::kill(pid, signal::SIGTERM)?;
            }

            if timeout(Duration::from_secs(2), process.wait())
                .await
                .is_err()
            {
                // Go for the kill
                process.kill().await?;
            }
        }

        // Block until process is freed
        process.wait().await.map_err(super::UnixIoError::from)
    }
}

#[cfg(all(test, unix))]
mod test {
    use crate::Spawner;

    #[tokio::test]
    async fn read() {
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
    async fn write() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut child = tokio::process::Command::new("cat").spawn_owned().unwrap();
        assert!(child.write_all(b"hello\n").await.is_ok());

        let mut buffer = [0_u8; 10];
        let bytes = child.read(&mut buffer).await.unwrap();
        assert_eq!("hello\n", std::str::from_utf8(&buffer[..bytes]).unwrap());
    }

    #[tokio::test]
    async fn test_drop_does_not_panic() {
        let child = tokio::process::Command::new("ls").spawn_owned().unwrap();
        let mut child = child.0;
        assert!(child.0.take().unwrap().shutdown().await.is_ok());
    }
}
