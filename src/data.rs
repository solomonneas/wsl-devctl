use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pm2Process {
    pub name: String,
    pub status: String,
    pub port: Option<u16>,
    pub memory_mb: u64,
    pub uptime_secs: Option<u64>,
    pub pid: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaddySite {
    pub label: String,
    pub root: String,
    pub port: Option<u16>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConflict {
    pub port: u16,
    pub owners: Vec<String>,
    pub is_open: bool,
}

pub async fn fetch_pm2_processes() -> Result<Vec<Pm2Process>> {
    let out = Command::new("pm2")
        .arg("jlist")
        .output()
        .await
        .context("failed to execute pm2 jlist")?;

    if !out.status.success() {
        return Ok(Vec::new());
    }

    let val: Value = serde_json::from_slice(&out.stdout).context("invalid pm2 jlist json")?;
    let arr = val.as_array().cloned().unwrap_or_default();

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64;

    let mut result = Vec::new();

    for p in arr {
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let pid = p.get("pid").and_then(Value::as_i64);

        let pm2_env = p.get("pm2_env").cloned().unwrap_or(Value::Null);
        let status = pm2_env
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let memory_bytes = p
            .get("monit")
            .and_then(|m| m.get("memory"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let memory_mb = memory_bytes / 1024 / 1024;

        let uptime_secs = pm2_env
            .get("pm_uptime")
            .and_then(Value::as_u64)
            .and_then(|uptime_ms_epoch| now_ms.checked_sub(uptime_ms_epoch))
            .map(|delta| delta / 1000);

        let port = extract_port(&pm2_env);

        result.push(Pm2Process {
            name,
            status,
            port,
            memory_mb,
            uptime_secs,
            pid,
        });
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

fn extract_port(pm2_env: &Value) -> Option<u16> {
    let candidates = [
        "PORT",
        "port",
        "VITE_PORT",
        "DEV_PORT",
        "NEXT_PORT",
        "npm_package_config_port",
    ];

    if let Some(env) = pm2_env.get("env") {
        for key in candidates {
            if let Some(port) = parse_port_from_value(env.get(key)) {
                return Some(port);
            }
        }
    }

    for key in ["PORT", "port"] {
        if let Some(port) = parse_port_from_value(pm2_env.get(key)) {
            return Some(port);
        }
    }

    None
}

fn parse_port_from_value(v: Option<&Value>) -> Option<u16> {
    match v {
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Some(Value::String(s)) => s.parse::<u16>().ok(),
        _ => None,
    }
}

pub async fn fetch_caddy_sites() -> Result<Vec<CaddySite>> {
    let mut sites = Vec::new();

    if let Ok(mut api_sites) = fetch_caddy_from_api().await {
        sites.append(&mut api_sites);
    }

    if let Ok(mut file_sites) = fetch_caddyfile_sites().await {
        sites.append(&mut file_sites);
    }

    sites.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(sites)
}

async fn fetch_caddy_from_api() -> Result<Vec<CaddySite>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()?;

    let val: Value = client
        .get("http://localhost:2019/config/")
        .send()
        .await?
        .json()
        .await?;

    let mut out = Vec::new();

    if let Some(servers) = val
        .get("apps")
        .and_then(|a| a.get("http"))
        .and_then(|h| h.get("servers"))
        .and_then(Value::as_object)
    {
        for (server_name, server_val) in servers {
            let listen_port = server_val
                .get("listen")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .and_then(parse_listen_port);

            if let Some(routes) = server_val.get("routes").and_then(Value::as_array) {
                collect_file_servers(routes, listen_port, &format!("api:{server_name}"), &mut out);
            }
        }
    }

    Ok(out)
}

fn collect_file_servers(routes: &[Value], port: Option<u16>, label_prefix: &str, out: &mut Vec<CaddySite>) {
    for route in routes {
        if let Some(handle) = route.get("handle").and_then(Value::as_array) {
            for h in handle {
                let handler = h.get("handler").and_then(Value::as_str).unwrap_or_default();
                if handler == "file_server" {
                    let root = h
                        .get("root")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            route
                                .get("match")
                                .and_then(Value::as_array)
                                .and_then(|m| m.first())
                                .and_then(|x| x.get("root"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("(unknown)")
                        .to_string();

                    let host = route
                        .get("match")
                        .and_then(Value::as_array)
                        .and_then(|m| m.first())
                        .and_then(|m| m.get("host"))
                        .and_then(Value::as_array)
                        .and_then(|h| h.first())
                        .and_then(Value::as_str)
                        .unwrap_or(label_prefix)
                        .to_string();

                    out.push(CaddySite {
                        label: host,
                        root,
                        port,
                        source: "caddy-api".to_string(),
                    });
                }

                if let Some(subroutes) = h.get("routes").and_then(Value::as_array) {
                    collect_file_servers(subroutes, port, label_prefix, out);
                }
            }
        }
    }
}

fn parse_listen_port(s: &str) -> Option<u16> {
    if let Some((_, p)) = s.rsplit_once(':') {
        return p.parse::<u16>().ok();
    }
    s.parse::<u16>().ok()
}

async fn fetch_caddyfile_sites() -> Result<Vec<CaddySite>> {
    let contents = tokio::fs::read_to_string("/etc/caddy/Caddyfile")
        .await
        .context("unable to read Caddyfile")?;

    let mut out = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_port: Option<u16> = None;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.ends_with('{') {
            let site = line.trim_end_matches('{').trim();
            current_label = Some(site.to_string());
            current_port = parse_site_port(site);
            continue;
        }

        if line.starts_with('}') {
            current_label = None;
            current_port = None;
            continue;
        }

        if line.starts_with("root ") {
            let root = line
                .strip_prefix("root")
                .unwrap_or_default()
                .trim()
                .trim_start_matches('*')
                .trim()
                .to_string();

            out.push(CaddySite {
                label: current_label
                    .clone()
                    .unwrap_or_else(|| "(unnamed-caddy-site)".to_string()),
                root,
                port: current_port,
                source: "caddyfile".to_string(),
            });
        }
    }

    Ok(out)
}

fn parse_site_port(site: &str) -> Option<u16> {
    if let Some((_, p)) = site.rsplit_once(':') {
        return p.parse::<u16>().ok();
    }
    None
}

pub async fn detect_conflicts(
    pm2: &[Pm2Process],
    caddy: &[CaddySite],
    manual_ports: &[u16],
) -> Result<Vec<PortConflict>> {
    let mut owners: BTreeMap<u16, Vec<String>> = BTreeMap::new();

    for p in pm2 {
        if let Some(port) = p.port {
            owners
                .entry(port)
                .or_default()
                .push(format!("{} (PM2)", p.name));
        }
    }

    for c in caddy {
        if let Some(port) = c.port {
            owners
                .entry(port)
                .or_default()
                .push(format!("{} (Caddy)", c.label));
        }
    }

    for port in manual_ports {
        owners
            .entry(*port)
            .or_default()
            .push("manual watch".to_string());
    }

    let mut to_probe = BTreeSet::new();
    for p in owners.keys() {
        to_probe.insert(*p);
    }

    let mut out = Vec::new();
    for port in to_probe {
        let is_open = is_port_open(port).await;
        let port_owners = owners.remove(&port).unwrap_or_default();
        let distinct_count = port_owners.len();

        if distinct_count > 1 || (port_owners.iter().any(|o| o == "manual watch") && is_open) {
            out.push(PortConflict {
                port,
                owners: port_owners,
                is_open,
            });
        }
    }

    Ok(out)
}

async fn is_port_open(port: u16) -> bool {
    timeout(
        Duration::from_millis(200),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

pub fn format_uptime(uptime_secs: Option<u64>) -> String {
    match uptime_secs {
        None => "-".to_string(),
        Some(s) if s < 60 => format!("{}s", s),
        Some(s) if s < 3600 => format!("{}m", s / 60),
        Some(s) if s < 86400 => format!("{}h", s / 3600),
        Some(s) => format!("{}d", s / 86400),
    }
}
