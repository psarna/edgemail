use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use std::env;
use std::error::Error;

// FIXME: wrap in a struct implementing a state machine
async fn handle_smtp(msg: &str) -> &'static [u8] {
    println!("Received {msg}");
    let mut msg = msg.split_whitespace();
    let command = msg.next().expect("empty message").to_lowercase();
    match command.as_str() {
        "ehlo" | "helo" => b"250\n",
        "mail" => {
            println!(
                "{}",
                msg.map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            b"250\n"
        }
        "rcpt" => {
            println!(
                "{}",
                msg.map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            b"250\n"
        }
        "data" => b"354\n",
        "quit" => b"221\n",
        _ => b"250\n",
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:8088".to_string());

    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        // Asynchronously wait for an inbound socket.
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            println!("Accepted");
            socket
                .write_all(b"220 eatmail\n")
                .await
                .expect("failed to introduce myself");
            let mut buf = vec![0; 65536];

            // In a loop, read data from the socket and write the data back.
            loop {
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("failed to read data from socket");

                if n == 0 {
                    println!("Received \\0");
                    return;
                }
                let msg = match std::str::from_utf8(&buf) {
                    Ok(msg) => msg,
                    Err(e) => {
                        println!("Unexpected response: {e}");
                        break;
                    }
                };
                let response = handle_smtp(msg).await;
                socket
                    .write_all(response)
                    .await
                    .expect("failed to write the response");
                if response == b"221\n" {
                    // fixme with enum
                    break;
                }
            }
        });
    }
}
