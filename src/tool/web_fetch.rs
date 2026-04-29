use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::Value;

use super::{truncate::truncate, Tool, ToolContext, ToolResult};

pub struct WebFetchTool;

impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch web page content via HTTP"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "url": "string (required) - http/https URL to fetch",
            "format": "string (optional) - 'text' (default, strips HTML) or 'html'"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        let raw_url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if raw_url.is_empty() {
            return Err("url is required".to_string());
        }
        if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") {
            return Err("only http/https URLs are supported".to_string());
        }

        let parsed = reqwest::Url::parse(raw_url).map_err(|_| "invalid URL")?;
        let host = parsed.host_str().ok_or("missing host")?;

        // Resolve DNS and check for private IPs
        let ips: Vec<String> = std::process::Command::new("getent")
            .args(["hosts", host])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                Some(
                    s.split_whitespace()
                        .filter(|token| token.parse::<IpAddr>().is_ok())
                        .map(|s| s.to_string())
                        .collect(),
                )
            })
            .unwrap_or_default();

        // Fallback: try std::net lookup
        let ips = if ips.is_empty() {
            (format!("{}:80", host), 0)
                .to_socket_addrs()
                .ok()
                .map(|addrs| addrs.map(|a| a.ip().to_string()).collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            ips
        };

        if ips.is_empty() {
            // Try reqwest DNS resolver as last resort
            // For now, let the request proceed and fail if DNS is bad
        } else {
            for ip_str in &ips {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    if is_private_ip(ip) {
                        return Err("requests to private/internal addresses are not allowed".to_string());
                    }
                }
            }
        }

        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let url = ctx.args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let format = ctx.args.get("format").and_then(|v| v.as_str()).unwrap_or("text");

        let rt = tokio::runtime::Handle::current();
        let url_owned = url.to_string();
        let format_owned = format.to_string();

        let result = rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| e.to_string())?;

            let resp = client.get(&url_owned).send().await.map_err(|e| e.to_string())?;
            let body = resp
                .bytes()
                .await
                .map_err(|e| e.to_string())?;

            // Limit to 1MB
            let body = if body.len() > 1024 * 1024 {
                &body[..1024 * 1024]
            } else {
                &body
            };

            let content = String::from_utf8_lossy(body).to_string();
            let content = if format_owned != "html" {
                strip_html(&content)
            } else {
                content
            };

            let (output, _) = truncate(&content);
            Ok::<String, String>(output)
        });

        match result {
            Ok(output) => ToolResult {
                status: "success",
                output,
                error: String::new(),
                stop_stream: false,
                start_time: Instant::now(),
                end_time: Some(Instant::now()),
            },
            Err(e) => ToolResult::error(e),
        }
    }
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            matches!(octets[0], 10) ||
            (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31) ||
            (octets[0] == 192 && octets[1] == 168) ||
            octets[0] == 127 ||
            (octets[0] == 169 && octets[1] == 254)
        }
        IpAddr::V6(v6) => v6.is_unicast_link_local() || v6.is_loopback() || {
            let segments = v6.segments();
            segments[0] >= 0xfc00
        },
    }
}

fn strip_html(s: &str) -> String {
    static HTML_TAG: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
    static MULTI_SPACE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[ \t]{2,}").unwrap());
    static MULTI_NEWLINE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

    let s = HTML_TAG.replace_all(s, " ");
    let s = MULTI_SPACE.replace_all(&s, " ");
    let s = MULTI_NEWLINE.replace_all(&s, "\n\n");
    s.trim().to_string()
}

use std::net::ToSocketAddrs;
