use crate::database::{Client, MailRecord};
use anyhow::Result;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout, Duration},
};

const MAX_SERVED_REQUESTS: usize = 100;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_SIZE: usize = 10;
static SERVED_REQUESTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InboxMessageSummary {
    pub id: i64,
    pub date: String,
    pub recipients: Vec<String>,
    pub sender: String,
    pub subject: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InboxMessage {
    pub id: i64,
    pub date: String,
    pub recipients: Vec<String>,
    pub sender: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InboxListResponse {
    pub mail: Vec<InboxMessageSummary>,
    pub has_more_pages: bool,
}

pub fn spawn(port: u16) {
    std::thread::spawn(move || -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?
            .block_on(async move {
                let local = tokio::task::LocalSet::new();
                local
                    .run_until(async move {
                        if let Err(err) = serve(port).await {
                            tracing::error!("Inbox API failed: {}", err);
                        }
                    })
                    .await;
            });
        Ok(())
    });
}

async fn serve(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Inbox API listening on: {}", addr);

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("Accepted API connection from {}", peer);
        tokio::task::spawn_local(async move {
            if let Err(err) = handle_connection(stream).await {
                tracing::warn!("API request failed: {}", err);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let request_number = SERVED_REQUESTS.fetch_add(1, Ordering::SeqCst);
    if request_number >= MAX_SERVED_REQUESTS {
        write_response(
            &mut stream,
            503,
            &error_body("service unavailable: request limit reached"),
        )
        .await?;
        return Ok(());
    }

    let mut buf = vec![0; 16 * 1024];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let Some(request_line) = request.lines().next() else {
        write_response(&mut stream, 400, &error_body("invalid request")).await?;
        return Ok(());
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    if method != "GET" {
        write_response(&mut stream, 405, &error_body("method not allowed")).await?;
        return Ok(());
    }

    match timeout(REQUEST_TIMEOUT, route_request(target)).await {
        Ok(Ok(body)) => write_response(&mut stream, 200, &body).await?,
        Ok(Err(ApiError { status, message })) => {
            write_response(&mut stream, status, &error_body(&message)).await?
        }
        Err(_) => write_response(&mut stream, 504, &error_body("request timed out")).await?,
    }

    Ok(())
}

async fn route_request(target: &str) -> Result<String, ApiError> {
    let (path, query) = split_target(target);
    match path {
        "/inbox" => list_inbox(query).await,
        _ => match path.strip_prefix("/inbox/") {
            Some(id) => get_inbox_message(id).await,
            None => Err(ApiError::not_found("not found")),
        },
    }
}

async fn list_inbox(query: &str) -> Result<String, ApiError> {
    let params = parse_query(query);
    let inbox = params
        .get("inbox")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("missing required query parameter: inbox"))?;
    let page = match params.get("page") {
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|page| *page > 0)
            .ok_or_else(|| ApiError::bad_request("page must be a positive integer"))?,
        None => 1,
    };
    let db = Client::new().await?;
    let rows = db.query_mail_by_recipient(inbox).await?;
    let total = rows.len();
    let start = (page - 1) * PAGE_SIZE;
    let end = start.saturating_add(PAGE_SIZE).min(total);
    let messages: Vec<InboxMessageSummary> = if start >= total {
        Vec::new()
    } else {
        rows[start..end]
            .iter()
            .map(|record| {
                let parsed = ParsedMail::from_raw(&record.data);
                InboxMessageSummary {
                    id: record.id,
                    date: record.date.clone(),
                    recipients: split_recipients(&record.recipients),
                    sender: record.sender.clone(),
                    subject: parsed.subject,
                }
            })
            .collect()
    };
    let response = InboxListResponse {
        mail: messages,
        has_more_pages: end < total,
    };
    serde_json::to_string(&response).map_err(Into::into)
}

async fn get_inbox_message(id: &str) -> Result<String, ApiError> {
    let id = id
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("invalid message id"))?;
    let db = Client::new().await?;
    let record = db
        .query_mail_by_id(id)
        .await?
        .ok_or_else(|| ApiError::not_found("message not found"))?;
    serde_json::to_string(&record_to_message(record)).map_err(Into::into)
}

fn record_to_message(record: MailRecord) -> InboxMessage {
    let parsed = ParsedMail::from_raw(&record.data);
    InboxMessage {
        id: record.id,
        date: record.date,
        recipients: split_recipients(&record.recipients),
        sender: record.sender,
        subject: parsed.subject,
        body: parsed.body,
    }
}

fn split_recipients(recipients: &str) -> Vec<String> {
    recipients
        .split(',')
        .map(str::trim)
        .filter(|recipient| !recipient.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedMail {
    subject: String,
    body: String,
}

impl ParsedMail {
    fn from_raw(raw: &str) -> Self {
        let normalized = raw.replace("\r\n", "\n");
        let (headers, body) = match normalized.split_once("\n\n") {
            Some((headers, body)) => (headers, body),
            None => ("", normalized.as_str()),
        };
        let subject = unfolded_headers(headers)
            .into_iter()
            .find_map(|header| {
                header
                    .strip_prefix("Subject:")
                    .or_else(|| header.strip_prefix("subject:"))
                    .map(|value| value.trim().to_string())
            })
            .unwrap_or_default();

        Self {
            subject,
            body: body.to_string(),
        }
    }
}

fn unfolded_headers(headers: &str) -> Vec<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in headers.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = unfolded.last_mut() {
                last.push(' ');
                last.push_str(line.trim());
            }
        } else {
            unfolded.push(line.to_string());
        }
    }
    unfolded
}

fn split_target(target: &str) -> (&str, &str) {
    target.split_once('?').unwrap_or((target, ""))
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode_component(key), decode_component(value))
        })
        .collect()
}

fn decode_component(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                decoded.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                    decoded.push(hex as char);
                    i += 3;
                } else {
                    decoded.push('%');
                    i += 1;
                }
            }
            byte => {
                decoded.push(byte as char);
                i += 1;
            }
        }
    }
    decoded
}

async fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

fn error_body(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

struct ApiError {
    status: u16,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("API error: {}", err);
        Self {
            status: 500,
            message: "internal server error".to_string(),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        tracing::error!("API serialization error: {}", err);
        Self {
            status: 500,
            message: "internal server error".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subject_and_body() {
        let parsed = ParsedMail::from_raw(
            "Subject: Test message\r\nFrom: sender@example.com\r\n\r\nHello world\r\nSecond line",
        );
        assert_eq!(parsed.subject, "Test message");
        assert_eq!(parsed.body, "Hello world\nSecond line");
    }

    #[test]
    fn unfolds_continued_subject_lines() {
        let parsed = ParsedMail::from_raw("Subject: Long\r\n subject\r\n\r\nBody");
        assert_eq!(parsed.subject, "Long subject");
    }

    #[test]
    fn returns_empty_subject_when_missing() {
        let parsed = ParsedMail::from_raw("From: sender@example.com\r\n\r\nHello world");
        assert_eq!(parsed.subject, "");
    }

    #[test]
    fn decodes_query_components() {
        let params = parse_query("inbox=agent%40example.com&unused=hello+world");
        assert_eq!(params.get("inbox"), Some(&"agent@example.com".to_string()));
        assert_eq!(params.get("unused"), Some(&"hello world".to_string()));
    }

    #[test]
    fn serializes_paginated_list_response_shape() {
        let response = InboxListResponse {
            mail: vec![InboxMessageSummary {
                id: 1,
                date: "2026-05-18 10:00:00.000".to_string(),
                recipients: vec!["<a@idont.date>".to_string()],
                sender: "<noreply@example.com>".to_string(),
                subject: "Hello".to_string(),
            }],
            has_more_pages: true,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            "{\"mail\":[{\"id\":1,\"date\":\"2026-05-18 10:00:00.000\",\"recipients\":[\"<a@idont.date>\"],\"sender\":\"<noreply@example.com>\",\"subject\":\"Hello\"}],\"has_more_pages\":true}"
        );
    }
}
