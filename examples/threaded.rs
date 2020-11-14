use pwner::Spawner;

/// A simnple sync example using threads to achieve concurrency
///
/// Using the synchronous interface, concurrently do:
/// - read `stdin` until `EOF` (CTRL + D) is reached
/// - write whatever was captured from `stdin` to child process
/// - write whatever is in the `stdout` of the child process
/// - write whatever is in the `stderr` of the child process
fn main() {
    // Use std::process::Command to create a new owned child
    let mut child = std::process::Command::new("cat")
        .spawn_owned()
        .expect("Couldn't start the child process");

    // Decompose the child into the three pipes
    let (stdin, stdout, stderr) = child.pipes();

    // Create a scope for the pipes, since they are `&mut`
    crossbeam::scope(|s| {
        // Stdout
        s.spawn(|_| {
            use std::io::Read;

            // Prepare a 1kb buffer
            let mut buffer = [0_u8; 1024];
            while let Ok(bytes) = stdout.read(&mut buffer) {
                // The child outputs to stdout with a '\n' and we are using `println!`
                // we should ignore the last `char`, since we already add a `\n`
                // This assumes that no line is longer than 1kb
                if bytes == 0 {
                    println!("out ::");
                } else if let Ok(string) = std::str::from_utf8(&buffer[..bytes - 1]) {
                    println!("out :: {}", string);
                }
            }
        });

        // Stderr
        s.spawn(|_| {
            use std::io::Read;

            // Prepare a 1kb buffer
            let mut buffer = [0_u8; 1024];
            while let Ok(bytes) = stderr.read(&mut buffer) {
                // The child outputs to stdout with a '\n' and we are using `println!`
                // we should ignore the last `char`, since we already add a `\n`
                // This assumes that no line is longer than 1kb
                if bytes == 0 {
                    eprintln!("err ::");
                } else if let Ok(string) = std::str::from_utf8(&buffer[..bytes - 1]) {
                    eprintln!("err :: {}", string);
                }
            }
        });

        // Stdin
        {
            use std::io::{Read, Write};

            // Prepare a 1kb buffer
            let mut buffer = [0_u8; 1024];

            // Must read at least one byte (or else we are at EOF)
            while let Ok(bytes @ 1..=1024) = std::io::stdin().read(&mut buffer) {
                // Write all the bytes that we got into the child
                stdin
                    .write_all(&buffer[..bytes])
                    .expect("Could not write to child");
            }
        }

        std::process::exit(0);
    })
    .expect("Failed to start threads");
}
