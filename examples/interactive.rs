struct Process {
    name: String,
    process: pwner::tokio::Process,
}

enum Command {
    List,
    Quit,
    Print(usize),
    Send(usize, String),
    Attach(usize),
    Run(String),
    Kill(usize),
    ProcessNotFound,
    Unknown,
}

fn parse(command: &str, children: &[Process]) -> Command {
    let mut args = command.split(' ');

    if let Some(first) = args.next() {
        match first {
            "l" => Command::List,
            "q" => Command::Quit,
            "p" => args
                .next()
                .and_then(|a| a.parse::<usize>().ok())
                .filter(|i| i < &children.len())
                .map_or(Command::ProcessNotFound, Command::Print),
            "a" => args
                .next()
                .and_then(|a| a.parse::<usize>().ok())
                .filter(|i| i < &children.len())
                .map_or(Command::ProcessNotFound, Command::Attach),
            "k" => args
                .next()
                .and_then(|a| a.parse::<usize>().ok())
                .filter(|i| i < &children.len())
                .map_or(Command::ProcessNotFound, Command::Kill),
            "s" => {
                if let Some(index) = args
                    .next()
                    .and_then(|a| a.parse::<usize>().ok())
                    .filter(|i| i < &children.len())
                {
                    if let Some(first) = args.next() {
                        let mut payload = String::from(first);
                        args.for_each(|arg| {
                            payload.push(' ');
                            payload.push_str(arg);
                        });
                        payload.push('\n');
                        Command::Send(index, payload)
                    } else {
                        Command::Unknown
                    }
                } else {
                    Command::ProcessNotFound
                }
            }
            "r" => {
                if let Some(command) = args.next() {
                    let mut payload = String::from(command);
                    args.for_each(|arg| {
                        payload.push(' ');
                        payload.push_str(arg);
                    });
                    return Command::Run(payload);
                }
                Command::Unknown
            }
            _ => Command::Unknown,
        }
    } else {
        Command::Unknown
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut children = Vec::<Process>::new();

    loop {
        use std::io::Write;
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncReadExt;

        print!("> ");
        let _ = std::io::stdout().flush();
        let mut buffer = String::new();
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
        match reader.read_line(&mut buffer).await {
            Ok(bytes) => {
                if bytes == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
        buffer.remove(buffer.len() - 1);

        match parse(&buffer, &children) {
            Command::List => children
                .iter()
                .enumerate()
                .for_each(|(i, process)| println!("{} - {}", i, process.name)),
            Command::Run(cmd) => {
                use pwner::Spawner;

                if let Ok(child) = tokio::process::Command::new(&cmd).spawn_owned() {
                    children.push(Process {
                        name: cmd,
                        process: child,
                    });
                } else {
                    eprintln!("Could not start \"{}\"", cmd);
                }
            }
            Command::Kill(index) => {
                children.remove(index);
            }
            Command::Quit => break,
            Command::Print(index) => {
                let process = &mut children[index].process;
                process.read_from(pwner::tokio::ReadSource::Both);

                let mut buffer = [0_u8; 1024];
                while let Ok(Ok(bytes)) = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    process.read(&mut buffer),
                )
                .await
                {
                    if let Ok(string) = std::str::from_utf8(&buffer[..bytes]) {
                        print!("{}", string);
                    }
                }
                let _ = std::io::stdout().flush();
            }
            Command::Send(index, payload) => {
                use tokio::io::AsyncWriteExt;
                if children[index]
                    .process
                    .write(payload.as_bytes())
                    .await
                    .is_err()
                {
                    eprintln!("Could not send to child");
                }
            }
            Command::Attach(index) => {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                let process = &mut children[index];

                let (stdin, stdout, stderr) = process.process.decompose();
                let mut stdin_buffer = [0u8; 1024];
                let mut stdout_buffer = [0_u8; 1024];
                let mut stderr_buffer = [0_u8; 1024];

                let mut input = tokio::io::stdin();
                println!("{} ----------", process.name);
                loop {
                    tokio::select! {
                        result = input.read(&mut stdin_buffer) => {
                            if let Ok(bytes @ 1..=1024) = result {
                                if stdin.write_all(&stdin_buffer[..bytes]).await.is_err() {
                                    break;
                                }
                            } else {
                                break;
                            }
                        },
                        Ok(bytes) = stdout.read(&mut stdout_buffer) => {
                            if bytes == 0 {
                                println!();
                            } else if let Ok(string) = std::str::from_utf8(&stdout_buffer[..bytes - 1]) {
                                println!("{}", string);
                            }
                        },
                        Ok(bytes) = stderr.read(&mut stderr_buffer) => {
                            if bytes == 0 {
                                eprintln!();
                            } else if let Ok(string) = std::str::from_utf8(&stderr_buffer[..bytes - 1]) {
                                eprintln!("{}", string);
                            }
                        },
                    }
                }
                println!("---------- {}", process.name);
            }
            Command::ProcessNotFound => eprintln!("Process is not running"),
            Command::Unknown => eprintln!("Unrecognized command"),
        }
    }
}
