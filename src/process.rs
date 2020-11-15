//! Holds the std implementation of a process.
//!
//! All interactions are blocking, including the dropping of child processes (on *nix platforms).
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
//! # Reading from the process
//!
//! ```no_run
//! # fn wrapper() -> Result<(), Box<dyn std::error::Error>> {
//! use std::io::Read;
//! use std::process::Command;
//! use pwner::Spawner;
//!
//! let mut child = Command::new("ls").spawn_owned()?;
//! let mut output = String::new();
//! child.read_to_string(&mut output)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Writing to the process
//!
//! ```no_run
//! # fn wrapper() -> Result<(), Box<dyn std::error::Error>> {
//! use std::io::{Read, Write};
//! use std::process::Command;
//! use pwner::Spawner;
//!
//! let mut child = Command::new("cat").spawn_owned()?;
//! child.write_all(b"hello\n")?;
//!
//! let mut buffer = [0_u8; 10];
//! child.read(&mut buffer)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Graceful dropping
//!
//! **Note:** Only available on *nix platforms.
//!
//! When the owned process gets dropped, [`Process`](../trait.Process.html) will try to
//! kill it gracefully by sending a `SIGINT` and checking, without blocking, if the child has diesd.
//! If the child is still running, it will block for 2seconds. If the process still doesn't die,
//! a `SIGTERM` is sent and another chance is given, until finally a `SIGKILL` is sent.

/// Possible sources to read from
#[derive(Debug, Copy, Clone)]
pub enum ReadSource {
    /// Read from the child's stdout
    Stdout,
    /// Read from the child's stderr
    Stderr,
}

/// An implementation of [`Process`](../trait.Process.html) that uses [`std::process`](std::process)
/// as the launcher.
///
/// All read and write operations are sync.
///
/// **Note:** On *nix platforms, the owned process will have 2 seconds between signals, which is a
/// blocking wait.
pub struct Duplex(Simplex, Output);

impl super::Spawner for std::process::Command {
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
                read_source: ReadSource::Stdout,
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
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_owned() {
    ///     println!("Child's ID is {}", child.id());
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    pub fn id(&self) -> u32 {
        self.0.id()
    }

    /// Choose which pipe to read form next.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::Read;
    /// use std::process::Command;
    /// use pwner::Spawner;
    /// use pwner::process::ReadSource;
    ///
    /// let mut child = Command::new("ls").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// child.read_from(ReadSource::Stdout).read(&mut buffer).unwrap();
    /// ```
    pub fn read_from(&mut self, read_source: ReadSource) -> &mut Self {
        self.1.read_from(read_source);
        self
    }

    /// Decomposes the handle into mutable references to the pipes.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::{ Read, Write };
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut child = Command::new("cat").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdin, stdout, _) = child.pipes();
    ///
    /// stdin.write_all(b"hello\n").unwrap();
    /// stdout.read(&mut buffer).unwrap();
    /// ```
    pub fn pipes(
        &mut self,
    ) -> (
        &mut std::process::ChildStdin,
        &mut std::process::ChildStdout,
        &mut std::process::ChildStderr,
    ) {
        (self.0.stdin(), &mut self.1.stdout, &mut self.1.stderr)
    }

    /// Separates the process and its input from the output pipes. Ownership is retained by a
    /// [`Simplex`](struct.Simplex.html) which still implements a graceful drop of the child process.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::{Read, Write};
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let child = Command::new("cat").spawn_owned().unwrap();
    /// let (mut input_only_process, mut output) = child.decompose();
    ///
    /// // Spawn printing thread
    /// std::thread::spawn(move || {
    ///     let mut buffer = [0; 1024];
    ///     while let Ok(bytes) = output.read(&mut buffer) {
    ///         if let Ok(string) = std::str::from_utf8(&buffer[..bytes]) {
    ///             print!("{}", string);
    ///         }
    ///     }
    /// });
    ///
    /// // Interact normally with the child process
    /// input_only_process.write_all(b"hello\n").unwrap();
    /// ```
    #[must_use]
    pub fn decompose(self) -> (Simplex, Output) {
        (self.0, self.1)
    }

    /// Completely releases the ownership of the child process. The raw underlying process and
    /// pipes are returned and no wrapping function is applicable any longer.
    ///
    /// **Note:** By ejecting the process, no graceful drop will be available any longer.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::{ Read, Write };
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut child = Command::new("cat").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// let (process, mut stdin, mut stdout, _) = child.eject();
    ///
    /// stdin.write_all(b"hello\n").unwrap();
    /// stdout.read(&mut buffer).unwrap();
    ///
    /// // Drop will not be executed for `child` as the ejected variable leaves scope here
    /// ```
    #[must_use]
    pub fn eject(
        self,
    ) -> (
        std::process::Child,
        std::process::ChildStdin,
        std::process::ChildStdout,
        std::process::ChildStderr,
    ) {
        let (process, stdin) = self.0.eject();
        let (stdout, stderr) = self.1.eject();
        (process, stdin, stdout, stderr)
    }
}

impl std::io::Write for Duplex {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl std::io::Read for Duplex {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.1.read(buf)
    }
}

/// An implementation of [`Process`](../trait.Process.html) that is stripped from any output
/// pipes.
///
/// All write operations are sync.
///
/// # Examples
///
/// ```no_run
/// use std::process::Command;
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
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut command = Command::new("ls");
    /// if let Ok(child) = command.spawn_owned() {
    ///     let (process, _) = child.decompose();
    ///     println!("Child's ID is {}", process.id());
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    #[must_use]
    pub fn id(&self) -> u32 {
        self.0.as_ref().unwrap().process.id()
    }

    fn stdin(&mut self) -> &mut std::process::ChildStdin {
        &mut self.0.as_mut().unwrap().stdin
    }

    /// Completely releases the ownership of the child process. The raw underlying process and
    /// pipes are returned and no wrapping function is applicable any longer.
    ///
    /// **Note:** By ejecting the process, no graceful drop will be available any longer.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::{ Read, Write };
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let (child, mut output) = Command::new("cat").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// let (process, mut stdin) = child.eject();
    ///
    /// stdin.write_all(b"hello\n").unwrap();
    /// output.read(&mut buffer).unwrap();
    ///
    /// // Drop will not be executed for `child` as the ejected variable leaves scope here
    /// ```
    #[must_use]
    pub fn eject(mut self) -> (std::process::Child, std::process::ChildStdin) {
        let process = self.0.take().unwrap();
        (process.process, process.stdin)
    }
}

impl std::ops::Drop for Simplex {
    fn drop(&mut self) {
        if self.0.is_some() {
            let process = self.0.take().unwrap();
            let _ = process.shutdown();
        }
    }
}

impl std::io::Write for Simplex {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.as_mut().unwrap().stdin.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.as_mut().unwrap().stdin.flush()
    }
}

/// A readable handle for both the stdout and stderr of the child process.
pub struct Output {
    read_source: ReadSource,
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
}

impl Output {
    /// Choose which pipe to read form next.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::io::Read;
    /// use std::process::Command;
    /// use pwner::Spawner;
    /// use pwner::process::ReadSource;
    ///
    /// let (process, mut output) = Command::new("ls").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// output.read_from(ReadSource::Stdout).read(&mut buffer).unwrap();
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
    /// use std::io::{ Read, Write };
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let (process, mut output) = Command::new("ls").spawn_owned().unwrap().decompose();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdout, stderr) = output.pipes();
    ///
    /// stdout.read(&mut buffer).unwrap();
    /// ```
    pub fn pipes(
        &mut self,
    ) -> (
        &mut std::process::ChildStdout,
        &mut std::process::ChildStderr,
    ) {
        (&mut self.stdout, &mut self.stderr)
    }

    /// Consumes this struct returning the containing pipes.
    #[must_use]
    pub fn eject(self) -> (std::process::ChildStdout, std::process::ChildStderr) {
        (self.stdout, self.stderr)
    }
}

impl std::io::Read for Output {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.read_source {
            ReadSource::Stdout => self.stdout.read(buf),
            ReadSource::Stderr => self.stderr.read(buf),
        }
    }
}

struct ProcessImpl {
    process: std::process::Child,
    stdin: std::process::ChildStdin,
}

impl ProcessImpl {
    // Allowed because we are already assuming *nix
    #[allow(clippy::cast_possible_wrap)]
    #[cfg(unix)]
    pub fn pid(&self) -> nix::unistd::Pid {
        nix::unistd::Pid::from_raw(self.process.id() as nix::libc::pid_t)
    }

    #[cfg(not(unix))]
    fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        self.process.kill();
        self.process.wait()
    }

    #[cfg(unix)]
    fn shutdown(mut self) -> Result<std::process::ExitStatus, super::UnixIoError> {
        use std::io::Write;

        // Copy pid
        let pid = self.pid();

        // Close stdin
        self.stdin.flush()?;
        std::mem::drop(self.stdin);

        if let Ok(status) = self.process.try_wait() {
            if status.is_none() {
                use nix::sys::{signal, wait};
                use wait::WaitStatus::Exited;

                let no_hang = Some(wait::WaitPidFlag::WNOHANG);

                // Try SIGINT
                signal::kill(pid, signal::SIGINT)?;

                std::thread::sleep(std::time::Duration::from_secs(2));
                if let Ok(Exited(_, _)) = wait::waitpid(pid, no_hang) {
                } else {
                    // Try SIGTERM
                    signal::kill(pid, signal::SIGTERM)?;

                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if let Ok(Exited(_, _)) = wait::waitpid(pid, no_hang) {
                    } else {
                        // Go for the kill
                        self.process.kill()?;
                    }
                }
            }
        }

        // Block until process is freed
        self.process.wait().map_err(super::UnixIoError::from)
    }
}

#[cfg(all(test, unix))]
mod test {
    use crate::Spawner;

    #[test]
    fn read() {
        use std::io::BufRead;

        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("echo hello")
            .spawn_owned()
            .unwrap();
        let mut output = String::new();
        let mut reader = std::io::BufReader::new(child);
        assert!(reader.read_line(&mut output).is_ok());

        assert_eq!("hello\n", output);
    }

    #[test]
    fn write() {
        use std::io::{BufRead, Write};

        let mut child = std::process::Command::new("cat").spawn_owned().unwrap();
        assert!(child.write_all(b"hello\n").is_ok());

        let mut output = String::new();
        let mut reader = std::io::BufReader::new(child);
        assert!(reader.read_line(&mut output).is_ok());

        assert_eq!("hello\n", output);
    }

    #[test]
    fn decompose() {
        use std::io::{BufRead, Write};

        let child = std::process::Command::new("cat").spawn_owned().unwrap();
        let (mut child, output) = child.decompose();
        assert!(child.write_all(b"hello\n").is_ok());

        let mut buffer = String::new();
        let mut reader = std::io::BufReader::new(output);
        assert!(reader.read_line(&mut buffer).is_ok());

        assert_eq!("hello\n", buffer);
    }

    #[test]
    fn drop() {
        let child = std::process::Command::new("ls").spawn_owned().unwrap();
        let mut simplex = child.0;
        assert!(!simplex.0.take().unwrap().shutdown().is_ok());
    }
}
