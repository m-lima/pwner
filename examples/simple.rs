use pwner::PipedSpawner;
use std::io::{Read, Write};

/// A simnple ping-pong example using `cat`
///
/// Using the synchronous interface, write a some bytes, then read some some bytes
/// from `stdin` until `EOF` (CTRL + D) is reached
fn main() {
    // Use std::process::Command to create a new piped child
    let mut child = std::process::Command::new("cat")
        .spawn_piped()
        .expect("Couldn't start the child process");

    // Prepare a 1kb buffer
    let mut buffer = [0_u8; 1024];

    // Must read at least one byte (or else we are at EOF)
    while let Ok(bytes @ 1..=1024) = std::io::stdin().read(&mut buffer) {
        // Write all the bytes that we got into the child
        child
            .write_all(&buffer[..bytes])
            .expect("Could not write to child");

        // Read from the child reusing the same buffer
        if let Ok(bytes) = child.read(&mut buffer) {
            // The child outputs to stdout with a '\n' and we are using `println!`
            // we should ignore the last `char`, since we already add a `\n`
            // This assumes that no line is longer than 1kb
            if bytes == 0 {
                println!("::");
            } else if let Ok(string) = std::str::from_utf8(&buffer[..bytes - 1]) {
                println!(":: {}", string);
            }
        }
    }
}
