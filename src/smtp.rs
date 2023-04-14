use anyhow::{Context, Result};
use libsql_client::{client::GenericClient, DatabaseClient, Statement};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Debug, Default)]
pub struct Mail {
    from: String,
    to: Vec<String>,
    data: String,
}

#[derive(Clone, Debug)]
enum State {
    Fresh,
    Greeted,
    ReceivingRcpt(Mail),
    ReceivingData(Mail),
}

/// SMTP server
pub struct Server {
    stream: tokio::net::TcpStream,
    state: State,
    db: GenericClient,
}

impl Server {
    const OH_HAI: &[u8] = b"220 eatmail\n";
    const KK: &[u8] = b"250 Ok\n";
    const KK_PLZ_LOGIN: &[u8] = b"250-smtp.idont.date Hello idont.date\n250 AUTH PLAIN LOGIN\n";
    const AUTH_OK: &[u8] = b"235 Ok\n";
    const SEND_DATA_PLZ: &[u8] = b"354 End data with <CR><LF>.<CR><LF>\n";
    const KTHXBYE: &[u8] = b"221 Bye\n";
    const HOLD_YOUR_HORSES: &[u8] = &[];

    /// Creates a new server from a connected stream
    pub async fn new(stream: tokio::net::TcpStream) -> Result<Self> {
        Ok(Self {
            stream,
            state: State::Fresh,
            db: libsql_client::new_client().await?,
        })
    }

    /// Runs the server loop, accepting and handling SMTP commands
    pub async fn serve(&mut self) -> Result<()> {
        let mut buf = vec![0; 65536];
        loop {
            self.init_db().await?;
            let n = self.stream.read(&mut buf).await?;

            if n == 0 {
                tracing::info!("Received EOF");
                self.handle_smtp("quit").await.ok();
                break;
            }
            let msg = std::str::from_utf8(&buf[0..n])?;
            let response = self.handle_smtp(msg).await?;
            if response != Server::HOLD_YOUR_HORSES {
                self.stream.write_all(response).await?;
            } else {
                tracing::debug!("Not responding, awaiting more data");
            }
            if response == Server::KTHXBYE {
                break;
            }
        }
        Ok(())
    }

    /// Sends the initial SMTP greeting
    pub async fn greet(&mut self) -> Result<()> {
        self.stream
            .write_all(Server::OH_HAI)
            .await
            .map_err(|e| e.into())
    }

    /// Handles a single SMTP command
    pub async fn handle_smtp(&mut self, raw_msg: &str) -> Result<&'static [u8]> {
        tracing::trace!("Received {raw_msg} in state {:?}", self.state);
        let mut msg = raw_msg.split_whitespace();
        let command = msg.next().context("received empty command")?.to_lowercase();
        let state = std::mem::replace(&mut self.state, State::Fresh);
        match (command.as_str(), state) {
            ("ehlo", State::Fresh) => {
                tracing::trace!("Sending AUTH info");
                self.state = State::Greeted;
                Ok(Server::KK_PLZ_LOGIN)
            }
            ("helo", State::Fresh) => {
                self.state = State::Greeted;
                Ok(Server::KK)
            }
            ("noop", _) | ("help", _) | ("info", _) | ("vrfy", _) | ("expn", _) => {
                tracing::trace!("Got {command}");
                Ok(Server::KK)
            }
            ("rset", _) => {
                self.state = State::Fresh;
                Ok(Server::KK)
            }
            ("auth", _) => {
                tracing::trace!("Acknowledging AUTH");
                Ok(Server::AUTH_OK)
            }
            ("mail", State::Greeted) => {
                tracing::trace!("Receiving MAIL");
                let from = msg.next().context("received empty MAIL")?;
                let from = from
                    .strip_prefix("FROM:")
                    .context("received incorrect MAIL")?;
                tracing::debug!("FROM: {from}");
                self.state = State::ReceivingRcpt(Mail {
                    from: from.to_string(),
                    ..Default::default()
                });
                Ok(Server::KK)
            }
            ("rcpt", State::ReceivingRcpt(mut mail)) => {
                tracing::trace!("Receiving rcpt");
                let to = msg.next().context("received empty RCPT")?;
                let to = to.strip_prefix("TO:").context("received incorrect RCPT")?;
                tracing::debug!("TO: {to}");
                mail.to.push(to.to_string());
                self.state = State::ReceivingRcpt(mail);
                Ok(Server::KK)
            }
            ("data", State::ReceivingRcpt(mail)) => {
                tracing::trace!("Receiving data");
                self.state = State::ReceivingData(mail);
                Ok(Server::SEND_DATA_PLZ)
            }
            ("quit", State::ReceivingData(mail)) => {
                tracing::trace!(
                    "Received data: FROM: {} TO:{} DATA:{}",
                    mail.from,
                    mail.to.join(", "),
                    mail.data
                );
                self.state = State::Fresh;
                self.replicate_to_db(mail).await?;
                Ok(Server::KTHXBYE)
            }
            ("quit", _) => {
                tracing::warn!("Received quit before getting any data");
                Ok(Server::KTHXBYE)
            }
            (_, State::ReceivingData(mut mail)) => {
                tracing::trace!("Receiving data");
                let resp = if raw_msg.ends_with("\r\n.\r\n") {
                    Server::KK
                } else {
                    Server::HOLD_YOUR_HORSES
                };
                mail.data += raw_msg;
                self.state = State::ReceivingData(mail);
                Ok(resp)
            }
            _ => anyhow::bail!(
                "Unexpected message received in state {:?}: {raw_msg}",
                self.state
            ),
        }
    }

    /// Initializes the database
    async fn init_db(&self) -> Result<()> {
        self.db.execute("CREATE TABLE IF NOT EXISTS mail (date text, sender text, recipients text, data text)").await.map(|_| ())
    }

    /// Replicates received mail to the database
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
