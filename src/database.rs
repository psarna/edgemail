use crate::smtp::Mail;
use anyhow::{Context, Result};
use libsql_client::{client::GenericClient, DatabaseClient, Statement};

pub struct Client {
    db: GenericClient,
}

impl Client {
    /// Creates a new database client.
    /// If the LIBSQL_CLIENT_URL environment variable is not set, a local database will be used.
    /// It's also possible to use a remote database by setting the LIBSQL_CLIENT_URL environment variable.
    /// The `mail` table will be automatically created if it does not exist.
    pub async fn new() -> Result<Self> {
        if std::env::var("LIBSQL_CLIENT_URL").is_err() {
            let mut db_path = std::env::temp_dir();
            db_path.push("eatmail.db");
            let db_path = db_path.display();
            tracing::warn!("LIBSQL_CLIENT_URL not set, using a default local database: {db_path}");
            std::env::set_var("LIBSQL_CLIENT_URL", format!("file://{db_path}"));
        }
        let db = libsql_client::new_client().await?;
        db.batch([
            "CREATE TABLE IF NOT EXISTS mail (date text, sender text, recipients text, data text)",
            "CREATE INDEX IF NOT EXISTS mail_date ON mail(date)",
            "CREATE INDEX IF NOT EXISTS mail_recipients ON mail(recipients)",
        ])
        .await?;
        Ok(Self { db })
    }

    /// Replicates received mail to the database
    pub async fn replicate(&self, mail: Mail) -> Result<()> {
        let now = chrono::offset::Utc::now()
            .format("%Y-%m-%d %H:%M:%S%.3f")
            .to_string();
        self.db
            .execute(Statement::with_args(
                "INSERT INTO mail VALUES (?, ?, ?, ?)",
                libsql_client::args!(now, mail.from, mail.to.join(", "), mail.data),
            ))
            .await
            .map(|_| ())
    }

    /// Cleans up old mail
    pub async fn delete_old_mail(&self) -> Result<()> {
        let now = chrono::offset::Utc::now();
        let yesterday = now - chrono::Duration::days(1);
        let yesterday = &yesterday.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let count: i64 = i64::try_from(
            self.db
                .execute(Statement::with_args(
                    "SELECT COUNT(*) FROM mail WHERE date < ?",
                    libsql_client::args!(yesterday),
                ))
                .await?
                .rows
                .first()
                .context("No rows returned from a COUNT(*) query")?
                .values
                .first()
                .context("No values returned from a COUNT(*) query")?,
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        tracing::debug!("Found {count} old mail");
        self.db
            .execute(Statement::with_args(
                "DELETE FROM mail WHERE date < ?",
                libsql_client::args!(yesterday),
            ))
            .await
            .ok();
        Ok(())
    }
}
