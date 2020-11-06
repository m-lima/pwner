use pwner::Spawner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A simnple async example using `cat` and tokio's single threaded executor
///
/// Using the asynchronous interface, concurrently do:
/// - read `stdin` until `EOF` (CTRL + D) is reached
/// - write whatever was captured from `stdin` to child process
/// - write whatever is in the `stdout` of the child process
/// - write whatever is in the `stderr` of the child process
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Use tokio::process::Command to create a new owned child
    let mut child = tokio::process::Command::new("cat")
        .spawn_owned()
        .expect("Couldn't start the child process");

    // Decompose the child into the three pipes
    let (stdin, stdout, stderr) = child.decompose();

    // Create one 1kb buffer per pipe
    let mut stdin_buffer = [0_u8; 1024];
    let mut stdout_buffer = [0_u8; 1024];
    let mut stderr_buffer = [0_u8; 1024];

    // Use tokio's async stdin
    let mut input = tokio::io::stdin();

    // Loop until we reach EOF on stdin
    loop {
        // Try all read operations and execute whatever is ready
        tokio::select! {
            result = input.read(&mut stdin_buffer) => {
                if let Ok(bytes @ 1..=1024) = result {

                    // Write all of the buffer to the child process
                    if stdin.write_all(&stdin_buffer[..bytes]).await.is_err() {
                        eprintln!("Could not write to child");
                        break;
                    }

                } else {
                    // Reached EOF, break the loop
                    break;
                }
            },
            Ok(bytes) = stdout.read(&mut stdout_buffer) => {
                // The child outputs to stdout with a '\n' and we are using `println!`
                // we should ignore the last `char`, since we already add a `\n`
                // This assumes that no line is longer than 1kb
                if bytes == 0 {
                    println!("out ::");
                } else if let Ok(string) = std::str::from_utf8(&stdout_buffer[..bytes - 1]) {
                    println!("out :: {}", string);
                }
            },
            Ok(bytes) = stderr.read(&mut stderr_buffer) => {
                if bytes == 0 {
                    eprintln!("err ::");
                } else if let Ok(string) = std::str::from_utf8(&stderr_buffer[..bytes - 1]) {
                    eprintln!("err :: {}", string);
                }
            },
            else => break,
        }
    }
}
