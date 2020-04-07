//! Holds the std implementation of a process.
//!
//! All interactions are blocking, including the dropping of child processes (on *nix platforms).
//!
//! # Spawning an owned tokio process
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
//! When the owned process gets dropped, [`Process`](trait.Process.html) will try to
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
pub struct Process(Option<ProcessImpl>, ReadSource);

impl crate::Spawner for std::process::Command {
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
            ReadSource::Stdout,
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
    /// use std::process::Command;
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
    /// use std::io::{ Read, Write };
    /// use std::process::Command;
    /// use pwner::Spawner;
    ///
    /// let mut child = Command::new("cat").spawn_owned().unwrap();
    /// let mut buffer = [0_u8; 1024];
    /// let (stdin, stdout, _) = child.decompose();
    ///
    /// stdin.write_all(b"hello\n").unwrap();
    /// stdout.read(&mut buffer).unwrap();
    /// ```
    pub fn decompose(
        &mut self,
    ) -> (
        &mut std::process::ChildStdin,
        &mut std::process::ChildStdout,
        &mut std::process::ChildStderr,
    ) {
        let handle = self.0.as_mut().unwrap();
        (&mut handle.stdin, &mut handle.stdout, &mut handle.stderr)
    }
}

impl std::ops::Drop for Process {
    fn drop(&mut self) {
        if self.0.is_some() {
            let process = self.0.take().unwrap();
            let _ = process.shutdown();
        }
    }
}

impl std::io::Write for Process {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.as_mut().unwrap().stdin.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.as_mut().unwrap().stdin.flush()
    }
}

impl std::io::Read for Process {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.1 {
            ReadSource::Stdout => self.0.as_mut().unwrap().stdout.read(buf),
            ReadSource::Stderr => self.0.as_mut().unwrap().stderr.read(buf),
        }
    }
}

struct ProcessImpl {
    process: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
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
    fn shutdown(mut self) -> Result<std::process::ExitStatus, crate::UnixIoError> {
        use std::io::Write;

        // Copy pid
        let pid = self.pid();

        // Close stdin
        self.stdin.flush()?;
        std::mem::drop(self.stdin);

        // Close outputs
        std::mem::drop(self.stdout);
        std::mem::drop(self.stderr);

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
        self.process.wait().map_err(crate::UnixIoError::from)
    }
}

#[cfg(all(test, unix))]
mod test {
    use crate::Spawner;

    #[test]
    fn test_read() {
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
    fn test_write() {
        use std::io::{BufRead, Write};

        let mut child = std::process::Command::new("cat").spawn_owned().unwrap();
        assert!(child.write_all(b"hello\n").is_ok());

        let mut output = String::new();
        let mut reader = std::io::BufReader::new(child);
        assert!(reader.read_line(&mut output).is_ok());

        assert_eq!("hello\n", output);
    }

    #[test]
    fn test_drop() {
        let mut child = std::process::Command::new("ls").spawn_owned().unwrap();
        assert!(!child.0.take().unwrap().shutdown().is_ok());
    }
}
