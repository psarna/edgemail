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

    let local = tokio::task::LocalSet::new();

    // Task for deleting old mail
    local.spawn_local(async move {
        let db = match eatmail::database::Client::new().await {
            Ok(db) => db,
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                return;
            }
        };
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            tracing::info!("Deleting old mail");
            if let Err(e) = db.delete_old_mail().await {
                tracing::error!("Failed to delete old mail: {}", e);
            }
        }
    });

    // Main loop: accept connections and spawn a task to handle them
    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::info!("Accepted a connection from {}", addr);

        local
            .run_until(async move {
                let conn = tokio::task::spawn_local(async move {
                    let mut smtp = smtp::Server::new(stream).await?;
                    smtp.greet().await?;
                    smtp.serve().await
                });
                conn.await
            })
            .await
            .ok();
    }
}
