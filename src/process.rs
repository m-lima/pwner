#[derive(Debug, Copy, Clone)]
pub enum ReadSource {
    Stdout,
    Stderr,
}

pub struct Process(Option<ProcessImpl>, ReadSource);

impl crate::PipedSpawner for std::process::Command {
    type Output = Process;

    fn spawn_piped(&mut self) -> std::io::Result<Self::Output> {
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

impl Process {
    #[must_use]
    pub fn id(&self) -> u32 {
        self.0.as_ref().unwrap().process.id()
    }

    #[cfg(unix)]
    #[must_use]
    pub fn pid(&self) -> nix::unistd::Pid {
        self.0.as_ref().unwrap().pid()
    }

    pub fn read_from(&mut self, read_source: ReadSource) -> &mut Self {
        self.1 = read_source;
        self
    }
}

impl std::ops::Drop for Process {
    fn drop(&mut self) {
        let process = self.0.take().unwrap();
        let _ = process.shutdown();
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
                        // Go for SIGKILL
                        signal::kill(pid, signal::SIGKILL)?;
                    }
                }
            }
        }

        // Block until process is freed
        self.process.wait().map_err(crate::UnixIoError::from)
    }
}
