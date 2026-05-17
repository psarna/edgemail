use crate::smtp::Mail;
use anyhow::{Context, Result};
use libsql_client::{client::GenericClient, DatabaseClient, Statement};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailRecord {
    pub id: i64,
    pub date: String,
    pub sender: String,
    pub recipients: String,
    pub data: String,
}

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
            db_path.push("edgemail.db");
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
        let a_week_ago = now - chrono::Duration::days(7);
        let a_week_ago = &a_week_ago.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        tracing::trace!("Deleting old mail from before {a_week_ago}");

        let count: i64 = i64::try_from(
            self.db
                .execute(Statement::with_args(
                    "SELECT COUNT(*) FROM mail WHERE date < ?",
                    libsql_client::args!(a_week_ago),
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
                libsql_client::args!(a_week_ago),
            ))
            .await
            .ok();
        Ok(())
    }
    pub async fn query_mail_by_recipient(&self, recipient: &str) -> Result<Vec<MailRecord>> {
        let stmt = Statement::with_args(
            "SELECT rowid, date, sender, recipients, data FROM mail WHERE recipients LIKE ? ORDER BY date DESC",
            libsql_client::args!(format!("%{}%", recipient))
        );
        let result = self.db.execute(stmt).await?;
        result
            .rows
            .into_iter()
            .map(Self::mail_record_from_row)
            .collect()
    }

    pub async fn query_mail_after_timestamp(
        &self,
        recipient: &str,
        timestamp: &str,
    ) -> Result<Vec<MailRecord>> {
        let stmt = Statement::with_args(
            "SELECT rowid, date, sender, recipients, data FROM mail WHERE recipients LIKE ? AND date >= ? ORDER BY date DESC",
            libsql_client::args!(format!("%{}%", recipient), timestamp)
        );
        let result = self.db.execute(stmt).await?;
        result
            .rows
            .into_iter()
            .map(Self::mail_record_from_row)
            .collect()
    }

    pub async fn query_mail_by_id(&self, id: i64) -> Result<Option<MailRecord>> {
        let stmt = Statement::with_args(
            "SELECT rowid, date, sender, recipients, data FROM mail WHERE rowid = ? LIMIT 1",
            libsql_client::args!(id),
        );
        let result = self.db.execute(stmt).await?;
        result
            .rows
            .into_iter()
            .next()
            .map(Self::mail_record_from_row)
            .transpose()
    }

    fn mail_record_from_row(row: libsql_client::Row) -> Result<MailRecord> {
        let mut values = row.values.into_iter();
        Ok(MailRecord {
            id: i64::try_from(values.next().context("mail row missing id")?)
                .map_err(|e| anyhow::anyhow!("{:?}", e))?,
            date: value_to_string(values.next().context("mail row missing date")?),
            sender: value_to_string(values.next().context("mail row missing sender")?),
            recipients: value_to_string(values.next().context("mail row missing recipients")?),
            data: value_to_string(values.next().context("mail row missing data")?),
        })
    }
}

fn value_to_string(value: libsql_client::Value) -> String {
    value.to_string().trim_matches('"').to_string()
}
