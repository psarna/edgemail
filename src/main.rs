use tokio::net::TcpListener;

use std::env;
use std::error::Error;

use eatmail::smtp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:2525".to_string());

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on: {}", addr);

    loop {
        let (stream, addr) = listener.accept().await?;

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                tokio::task::spawn_local(async move {
                    tracing::info!("Accepted {}", addr);
                    let mut smtp = smtp::Server::new(stream).await?;
                    smtp.greet().await?;
                    smtp.serve().await
                })
                .await?
            })
            .await?;
    }
}
