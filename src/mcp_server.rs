use anyhow::Result;
use edgemail::database::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};
use tokio::time::{sleep, Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
struct MCPRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MCPResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<MCPError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MCPError {
    code: i32,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GetTempAddressArgs {
    #[serde(default)]
    prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReadEmailsArgs {
    address: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WaitForEmailArgs {
    address: String,
    timestamp: String,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Email {
    date: String,
    sender: String,
    recipients: String,
    data: String,
}

struct MCPServer {
    db: Client,
    domain: String,
}

impl MCPServer {
    async fn new() -> Result<Self> {
        let domain = env::var("EDGEMAIL_DOMAIN").unwrap_or_else(|_| "idont.date".to_string());
        let db = Client::new().await?;
        Ok(Self { db, domain })
    }

    async fn handle_request(&self, request: MCPRequest) -> Option<MCPResponse> {
        // Handle notifications (methods starting with "notifications/") - these are ignored
        if request.method.starts_with("notifications/") {
            return None;
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(request.params).await,
            _ => Err(anyhow::anyhow!("Unknown method: {}", request.method)),
        };

        Some(match result {
            Ok(result) => MCPResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(err) => MCPResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(MCPError {
                    code: -32603,
                    message: err.to_string(),
                }),
            },
        })
    }

    async fn handle_initialize(&self, _params: Option<Value>) -> Result<Value> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "edgemail-mcp",
                "version": "0.2.3"
            }
        }))
    }

    async fn handle_tools_list(&self) -> Result<Value> {
        Ok(json!({
            "tools": [
                {
                    "name": "get_temp_address",
                    "description": "Generate a temporary email address with timestamp",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "prefix": {
                                "type": "string",
                                "description": "Optional prefix for the email address (default: 'agent')"
                            }
                        }
                    }
                },
                {
                    "name": "read_emails",
                    "description": "Read all emails for a given address",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "address": {
                                "type": "string",
                                "description": "Email address to read emails for"
                            }
                        },
                        "required": ["address"]
                    }
                },
                {
                    "name": "wait_for_email",
                    "description": "Wait for a new email to arrive after a given timestamp",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "address": {
                                "type": "string",
                                "description": "Email address to monitor"
                            },
                            "timestamp": {
                                "type": "string",
                                "description": "Timestamp to wait for emails after (ISO format)"
                            },
                            "timeout_seconds": {
                                "type": "integer",
                                "description": "Maximum seconds to wait for new email"
                            }
                        },
                        "required": ["address", "timestamp", "timeout_seconds"]
                    }
                }
            ]
        }))
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let tool_name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;
        let arguments = params["arguments"].clone();

        match tool_name {
            "get_temp_address" => self.get_temp_address(arguments).await,
            "read_emails" => self.read_emails(arguments).await,
            "wait_for_email" => self.wait_for_email(arguments).await,
            _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
        }
    }

    async fn get_temp_address(&self, args: Value) -> Result<Value> {
        let args: GetTempAddressArgs = serde_json::from_value(args)?;
        let prefix = args.prefix.unwrap_or_else(|| "agent".to_string());
        let timestamp = chrono::Utc::now().timestamp();
        let address = format!("{}_{timestamp}@{}", prefix, self.domain);
        
        Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": format!("Generated temporary email address: {}", address)
                }
            ],
            "address": address
        }))
    }

    async fn read_emails(&self, args: Value) -> Result<Value> {
        let args: ReadEmailsArgs = serde_json::from_value(args)?;
        
        let rows = self.db.query_mail_by_recipient(&args.address).await?;

        let emails: Vec<Email> = rows
            .into_iter()
            .map(|row| Email {
                date: row[0].to_string().trim_matches('"').to_string(),
                sender: row[1].to_string().trim_matches('"').to_string(),
                recipients: row[2].to_string().trim_matches('"').to_string(),
                data: row[3].to_string().trim_matches('"').to_string(),
            })
            .collect();

        Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": format!("Found {} emails for address {}", emails.len(), args.address)
                }
            ],
            "emails": emails
        }))
    }

    async fn wait_for_email(&self, args: Value) -> Result<Value> {
        let args: WaitForEmailArgs = serde_json::from_value(args)?;
        let start_time = Instant::now();
        let timeout = Duration::from_secs(args.timeout_seconds);
        
        loop {
            if start_time.elapsed() >= timeout {
                return Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Timeout reached: no new emails found for {} after {}", args.address, args.timestamp)
                        }
                    ],
                    "timeout": true,
                    "emails": []
                }));
            }

            let rows = self.db.query_mail_after_timestamp(&args.address, &args.timestamp).await?;

            if !rows.is_empty() {
                let emails: Vec<Email> = rows
                    .into_iter()
                    .map(|row| Email {
                        date: row[0].to_string().trim_matches('"').to_string(),
                        sender: row[1].to_string().trim_matches('"').to_string(),
                        recipients: row[2].to_string().trim_matches('"').to_string(),
                        data: row[3].to_string().trim_matches('"').to_string(),
                    })
                    .collect();

                return Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Found {} new emails for {} after {}", emails.len(), args.address, args.timestamp)
                        }
                    ],
                    "timeout": false,
                    "emails": emails
                }));
            }

            sleep(Duration::from_secs(1)).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let server = MCPServer::new().await?;
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    
    loop {
        let mut line = String::new();
        match stdin_lock.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Ok(request) = serde_json::from_str::<MCPRequest>(&line) {
                    if let Some(response) = server.handle_request(request).await {
                        let response_json = serde_json::to_string(&response)?;
                        println!("{}", response_json);
                        io::stdout().flush()?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}