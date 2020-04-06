#[derive(Debug, Copy, Clone)]
pub enum ReadSource {
    Stdout,
    Stderr,
    Both,
}

pub struct Process(Option<ProcessImpl>, ReadSource);

impl crate::PipedSpawner for tokio::process::Command {
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
            ReadSource::Both,
        ))
    }
}

impl std::ops::Drop for Process {
    fn drop(&mut self) {
        let process = self.0.take().unwrap();
        let _ = process.shutdown();
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
                    println!("Got stderr");
                    stderr
                } else {
                    println!("Trying stdout");
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
    async fn shutdown(mut self) -> Result<std::process::ExitStatus, crate::UnixIoError> {
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
