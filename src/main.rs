use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use libsql_client::{reqwest::Connection as DbConnection, Connection, Statement};

use std::env;
use std::error::Error;

#[derive(Clone, Debug, Default)]
struct Mail {
    from: String,
    to: Vec<String>,
    data: String,
}

#[derive(Clone, Debug)]
enum SmtpState {
    Fresh,
    Greeted,
    ReceivingRcpt(Mail),
    ReceivingData(Mail),
}

struct SmtpServer {
    stream: tokio::net::TcpStream,
    state: SmtpState,
    db: DbConnection,
}

impl SmtpServer {
    const HAI: &[u8] = b"220 eatmail\n";
    const KK: &[u8] = b"250\n";
    const SEND_DATA_PLZ: &[u8] = b"354\n";
    const KTHXBYE: &[u8] = b"221\n";

    fn new(stream: tokio::net::TcpStream) -> Result<Self> {
        Ok(Self {
            stream,
            state: SmtpState::Fresh,
            db: DbConnection::connect_from_env()?,
        })
    }

    async fn greet(&mut self) -> Result<()> {
        self.stream
            .write_all(SmtpServer::HAI)
            .await
            .map_err(|e| e.into())
    }

    async fn serve(&mut self) -> Result<()> {
        let mut buf = vec![0; 65536];
        loop {
            self.init_db().await?;
            let n = self.stream.read(&mut buf).await?;

            if n == 0 {
                tracing::info!("Received EOF");
                break;
            }
            let msg = std::str::from_utf8(&buf[0..n])?;
            let response = self.handle_smtp(msg).await?;
            self.stream.write_all(response).await?;
            if response == SmtpServer::KTHXBYE {
                break;
            }
        }
        Ok(())
    }

    async fn handle_smtp(&mut self, raw_msg: &str) -> Result<&'static [u8]> {
        tracing::trace!("Received {raw_msg} in state {:?}", self.state);
        let mut msg = raw_msg.split_whitespace();
        let command = msg
            .next()
            .ok_or(anyhow::anyhow!("received empty command"))?;
        let state = std::mem::replace(&mut self.state, SmtpState::Fresh);
        match (command, state) {
            ("ehlo", SmtpState::Fresh) | ("helo", SmtpState::Fresh) => {
                self.state = SmtpState::Greeted;
                Ok(SmtpServer::KK)
            }
            ("mail", SmtpState::Greeted) => {
                let from = msg.next().ok_or(anyhow::anyhow!("received empty MAIL"))?;
                let from = from
                    .strip_prefix("FROM:")
                    .ok_or(anyhow::anyhow!("received incorrect MAIL"))?;
                tracing::debug!("FROM: {from}");
                self.state = SmtpState::ReceivingRcpt(Mail {
                    from: from.to_string(),
                    ..Default::default()
                });
                Ok(SmtpServer::KK)
            }
            ("rcpt", SmtpState::ReceivingRcpt(mut mail)) => {
                let to = msg.next().ok_or(anyhow::anyhow!("received empty RCPT"))?;
                let to = to
                    .strip_prefix("TO:")
                    .ok_or(anyhow::anyhow!("received incorrect RCPT"))?;
                tracing::debug!("TO: {to}");
                mail.to.push(to.to_string());
                self.state = SmtpState::ReceivingRcpt(mail);
                Ok(SmtpServer::KK)
            }
            ("data", SmtpState::ReceivingRcpt(mail)) => {
                self.state = SmtpState::ReceivingData(mail);
                Ok(SmtpServer::SEND_DATA_PLZ)
            }
            ("quit", SmtpState::ReceivingData(mail)) => {
                tracing::debug!(
                    "Received data:\n{}\n{}\n{}",
                    mail.from,
                    mail.to.join(", "),
                    mail.data
                );
                self.state = SmtpState::Fresh;
                self.replicate_to_db(mail).await?;
                Ok(SmtpServer::KTHXBYE)
            }
            ("quit", _) => {
                tracing::warn!("Received quit before getting any data");
                Ok(SmtpServer::KTHXBYE)
            }
            (_, SmtpState::ReceivingData(mut mail)) => {
                let raw_msg = raw_msg.replace('\x0d', "").replace('\x0a', r"\n");
                mail.data += &raw_msg;
                self.state = SmtpState::ReceivingData(mail);
                Ok(SmtpServer::KK)
            }
            _ => Err(anyhow::anyhow!("Unexpected message received in state {:?}: {raw_msg}", self.state)),
        }
    }

    async fn init_db(&self) -> Result<()> {
        self.db.execute("CREATE TABLE IF NOT EXISTS mail (date text, sender text, recipients text, data text)").await.map(|_| ())
    }

    async fn replicate_to_db(&self, mail: Mail) -> Result<()> {
        let now = format!("{}", chrono::offset::Utc::now());
        self.db
            .execute(Statement::with_params(
                "INSERT INTO mail VALUES (?, ?, ?, ?)",
                libsql_client::params!(now, mail.from, mail.to.join(", "), mail.data),
            ))
            .await
            .map(|_| ())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:2525".to_string());

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on: {}", addr);

    loop {
        // Asynchronously wait for an inbound socket.
        let (stream, addr) = listener.accept().await?;

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                tokio::task::spawn_local(async move {
                    tracing::info!("Accepted {}", addr);
                    let mut smtp = SmtpServer::new(stream)?;
                    smtp.greet().await?;
                    smtp.serve().await
                })
                .await?
            })
            .await?;
    }
}
