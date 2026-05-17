use anyhow::{Context, Result};
use tokio::net::TcpListener;

use std::env;

use edgemail::{api, smtp};

struct Args {
    smtp_addr: String,
    domain: String,
    api_port: Option<u16>,
}

impl Args {
    fn parse() -> Result<Option<Self>> {
        let mut smtp_addr = None;
        let mut domain = None;
        let mut api_port = None;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print_usage();
                    return Ok(None);
                }
                "--api-port" => {
                    let value = args.next().context("--api-port requires a port number")?;
                    api_port = Some(value.parse().context("invalid value for --api-port")?);
                }
                value if value.starts_with("--api-port=") => {
                    let (_, port) = value.split_once('=').context("invalid --api-port format")?;
                    api_port = Some(port.parse().context("invalid value for --api-port")?);
                }
                _ if smtp_addr.is_none() => smtp_addr = Some(arg),
                _ if domain.is_none() => domain = Some(arg),
                _ => anyhow::bail!("unexpected argument: {arg}"),
            }
        }

        Ok(Some(Self {
            smtp_addr: smtp_addr.unwrap_or_else(|| "0.0.0.0:2525".to_string()),
            domain: domain.unwrap_or_else(|| "smtp.idont.date".to_string()),
            api_port,
        }))
    }
}

fn print_usage() {
    println!(
        "Usage: edgemail [SMTP_ADDR] [DOMAIN] [--api-port PORT]\n\
         \n\
         Arguments:\n\
           SMTP_ADDR        SMTP bind address (default: 0.0.0.0:2525)\n\
           DOMAIN           SMTP domain name (default: smtp.idont.date)\n\
         \n\
         Options:\n\
           --api-port PORT  Enable the inbox HTTP API on the given port\n\
           -h, --help       Print help"
    );
}

/// A helper function for cleaning up old mail from the database
fn periodically_clean_db(period: tokio::time::Duration) {
    std::thread::spawn(move || -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()
            .context("failed to build async runtime")?
            .block_on(async move {
                let local = tokio::task::LocalSet::new();
                local.spawn_local(async move {
                    let db = match edgemail::database::Client::new().await {
                        Ok(db) => db,
                        Err(e) => {
                            tracing::error!("Failed to connect to database: {}", e);
                            return;
                        }
                    };
                    let mut interval = tokio::time::interval(period);
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        interval.tick().await;
                        if let Err(e) = db.delete_old_mail().await {
                            tracing::error!("Failed to delete old mail: {}", e);
                        }
                    }
                });
                local.await;
            });
        Ok(())
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let Some(args) = Args::parse()? else {
        return Ok(());
    };
    let domain = args.domain.clone();

    tracing::info!("edgemail server for {domain} started");

    let listener = TcpListener::bind(&args.smtp_addr).await?;
    tracing::info!("Listening on: {}", args.smtp_addr);

    // Task for deleting old mail
    periodically_clean_db(tokio::time::Duration::from_secs(3600));

    if let Some(api_port) = args.api_port {
        api::spawn(api_port);
    }

    // Main loop: accept connections and spawn a task to handle them
    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::info!("Accepted a connection from {}", addr);
        let connection_domain = domain.clone();

        tokio::task::LocalSet::new()
            .run_until(async move {
                let smtp = smtp::Server::new(&connection_domain, stream).await?;
                tokio::time::timeout(std::time::Duration::from_secs(300), smtp.serve())
                    .await
                    .context("connection timed out")
            })
            .await
            .ok();
    }
}
