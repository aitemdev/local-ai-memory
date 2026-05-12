use crate::daemon::{read_info, DaemonInfo};
use anyhow::Result;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

pub fn endpoint() -> Option<String> {
    read_info().map(|info: DaemonInfo| format!("http://127.0.0.1:{}", info.port))
}

fn client() -> Client {
    Client::builder()
        .timeout(TIMEOUT)
        .build()
        .expect("reqwest client")
}

pub fn get(path: &str) -> Result<Value> {
    let base = endpoint().ok_or_else(|| anyhow::anyhow!("daemon not running"))?;
    let url = format!("{base}{path}");
    let response = client().get(&url).send()?.error_for_status()?;
    Ok(response.json()?)
}

pub fn post<B: Serialize>(path: &str, body: &B) -> Result<Value> {
    let base = endpoint().ok_or_else(|| anyhow::anyhow!("daemon not running"))?;
    let url = format!("{base}{path}");
    let response = client().post(&url).json(body).send()?.error_for_status()?;
    Ok(response.json()?)
}

pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}
