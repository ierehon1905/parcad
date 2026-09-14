//! `parcad mcp`: the MCP server over stdio, for clients that launch their
//! servers rather than connect to them — plugins, extension bundles, registry
//! installs.
//!
//! It is a relay, not a second server. Every line from stdin is POSTed to the
//! host on the port and every message in the reply is written back as a line,
//! so the tools, the instructions and the live session are `mcp.rs`'s own and
//! an agent launched this way shares the screen with the app window.
//!
//! When nothing is listening, this process becomes the host: it seeds the
//! project folder, hosts the UI, API and MCP on the port exactly as
//! `parcad serve` does, and relays to itself. The host lives as long as the
//! client keeps stdin open. A relay whose host goes away mid-session takes the
//! port over the same way, so two clients sharing one host survive the first
//! one quitting.
//!
//! stdout carries JSON-RPC and nothing else; everything for a person is on
//! stderr, which clients keep in their server log.

use crate::call::{exchange, Exchange, Reply};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a client speaks before `initialize` has settled it; the host treats a
/// request without the header as this version.
const LEGACY_PROTOCOL: &str = "2025-03-26";

struct Relay {
    port: u16,
    /// Negotiated in `initialize`, sent on every request after it.
    protocol: Mutex<String>,
    /// The host's `Mcp-Session-Id`, for clients on a protocol that keeps one.
    session: Mutex<Option<String>>,
    stdout: Mutex<std::io::Stdout>,
    /// The client's `initialize`, replayed whenever the host no longer knows
    /// the session, so a session-keeping client is not left holding a dead id.
    handshake: Mutex<Option<Value>>,
}

pub fn run(args: impl Iterator<Item = String>) -> Result<()> {
    let mut port = parcad_host::http::port();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .context("--port needs a number")?
                    .parse()
                    .context("--port must be a port number")?
            }
            other => anyhow::bail!("unknown argument {other:?}; usage: parcad mcp [--port N]"),
        }
    }

    let relay = Arc::new(Relay {
        port,
        protocol: Mutex::new(LEGACY_PROTOCOL.into()),
        session: Mutex::new(None),
        stdout: Mutex::new(std::io::stdout()),
        handshake: Mutex::new(None),
    });
    if listening(port) {
        eprintln!("parcad mcp: relaying to the parcad already running on 127.0.0.1:{port}");
    } else {
        relay.take_over()?;
    }

    // One thread per message: a long evaluation must not hold up a ping or the
    // cancellation of that same evaluation.
    let mut inflight = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let line = line.context("reading a message from stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(e) => {
                relay.write(&json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": format!("not JSON: {e}") },
                }));
                continue;
            }
        };
        // `initialize` settles the protocol and session every later request
        // carries, so nothing may be sent until its reply is in.
        if message["method"] == "initialize" {
            relay.forward(message);
            continue;
        }
        inflight.retain(|t: &std::thread::JoinHandle<()>| !t.is_finished());
        let relay = relay.clone();
        inflight.push(std::thread::spawn(move || relay.forward(message)));
    }
    for thread in inflight {
        let _ = thread.join();
    }
    relay.hang_up();
    Ok(())
}

impl Relay {
    fn forward(&self, message: Value) {
        if message["method"] == "initialize" {
            if let Some(version) = message["params"]["protocolVersion"].as_str() {
                *self.protocol.lock().unwrap() = version.to_string();
            }
            *self.handshake.lock().unwrap() = Some(message.clone());
        }
        let is_request = message.get("id").is_some() && message.get("method").is_some();
        let reply = match self.send(&message) {
            Ok(reply) => reply,
            Err(e) => {
                if is_request {
                    self.write(&json!({
                        "jsonrpc": "2.0", "id": message["id"],
                        "error": { "code": -32603, "message": format!("{e:#}") },
                    }));
                } else {
                    eprintln!("parcad mcp: {e:#}");
                }
                return;
            }
        };
        if let Some(session) = reply.header("mcp-session-id") {
            *self.session.lock().unwrap() = Some(session);
        }
        let messages = reply.messages().unwrap_or_default();
        for reply_message in &messages {
            if message["method"] == "initialize" {
                if let Some(version) = reply_message["result"]["protocolVersion"].as_str() {
                    *self.protocol.lock().unwrap() = version.to_string();
                }
            }
            self.write(reply_message);
        }
        // A refusal the host did not phrase as JSON-RPC still owes the client
        // an answer, or it waits on that id forever.
        let answered = messages.iter().any(|m| m.get("id") == message.get("id"));
        if is_request && !answered {
            let detail = match reply.code {
                200..=299 => "the host sent no reply".to_string(),
                code => format!("the host answered {code}: {}", reply.text.trim()),
            };
            self.write(&json!({
                "jsonrpc": "2.0", "id": message["id"],
                "error": { "code": -32603, "message": detail },
            }));
        }
    }

    fn send(&self, message: &Value) -> Result<Reply> {
        let attempt = || {
            let protocol = self.protocol.lock().unwrap().clone();
            let session = self.session.lock().unwrap().clone();
            exchange(self.port, "POST", message, &protocol, session.as_deref())
        };
        let had_session = self.session.lock().unwrap().is_some();
        match attempt() {
            // The host forgets a session left idle past rmcp's keep-alive, five
            // minutes by default, while the client still holds its id.
            Ok(reply)
                if reply.code == 404
                    && had_session
                    && message["method"] != "initialize"
                    && reply.text.contains("Session not found") =>
            {
                eprintln!("parcad mcp: the host no longer knows this session");
                self.reinitialize()?;
                attempt().map_err(|e| match e {
                    Exchange::Unreachable(address) => anyhow::anyhow!("no parcad answers on {address}"),
                    Exchange::Failed(e) => e,
                })
            }
            Ok(reply) => Ok(reply),
            Err(Exchange::Failed(e)) => Err(e),
            Err(Exchange::Unreachable(address)) => {
                eprintln!("parcad mcp: the host on {address} went away");
                self.take_over()?;
                if message["method"] != "initialize" {
                    self.reinitialize()?;
                }
                match attempt() {
                    Ok(reply) => Ok(reply),
                    Err(Exchange::Failed(e)) => Err(e),
                    Err(Exchange::Unreachable(address)) => {
                        anyhow::bail!("no parcad answers on {address}, even after starting one")
                    }
                }
            }
        }
    }

    fn take_over(&self) -> Result<()> {
        ensure_host(self.port, "parcad mcp", "until the client disconnects")
    }

    /// Repeat the client's handshake, answering nobody, so the requests after
    /// it carry a session the host knows.
    fn reinitialize(&self) -> Result<()> {
        let Some(handshake) = self.handshake.lock().unwrap().clone() else {
            return Ok(());
        };
        let Some(stale) = self.session.lock().unwrap().take() else {
            return Ok(());
        };
        let protocol = self.protocol.lock().unwrap().clone();
        let replay = |body: &Value, session: Option<&str>| {
            exchange(self.port, "POST", body, &protocol, session).map_err(|e| match e {
                Exchange::Unreachable(address) => anyhow::anyhow!("no parcad answers on {address}"),
                Exchange::Failed(e) => e,
            })
        };
        let reply = replay(&handshake, None)?;
        let fresh = reply.header("mcp-session-id");
        replay(
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            fresh.as_deref(),
        )?;
        eprintln!(
            "parcad mcp: session {stale} is gone; continuing as {}",
            fresh.as_deref().unwrap_or("a new one")
        );
        *self.session.lock().unwrap() = fresh;
        Ok(())
    }

    /// Close the host's session, so the window stops showing an agent that
    /// has gone.
    fn hang_up(&self) {
        let session = self.session.lock().unwrap().clone();
        if let Some(session) = session {
            let protocol = self.protocol.lock().unwrap().clone();
            let _ = exchange(self.port, "DELETE", &Value::Null, &protocol, Some(&session));
        }
    }

    fn write(&self, message: &Value) {
        let mut stdout = self.stdout.lock().unwrap();
        let _ = serde_json::to_writer(&mut *stdout, message);
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
    }
}

/// Make sure a host answers on `port`: the one already running, or one this
/// process starts on a thread of its own and keeps for as long as it lives.
/// `who` and `lifetime` only word the line on stderr.
pub fn ensure_host(port: u16, who: &str, lifetime: &str) -> Result<()> {
    static HOSTING: Mutex<bool> = Mutex::new(false);
    if listening(port) {
        return Ok(());
    }
    {
        let mut hosting = HOSTING.lock().unwrap();
        if !*hosting {
            *hosting = true;
            eprintln!("{who}: no parcad on 127.0.0.1:{port}; hosting one in this process {lifetime}");
            let who = who.to_string();
            std::thread::spawn(move || {
                if let Err(e) = crate::host(port) {
                    eprintln!("{who}: {e:#}");
                }
            });
        }
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !listening(port) {
        if Instant::now() > deadline {
            anyhow::bail!(
                "started a host on 127.0.0.1:{port} but it never answered; stderr above says \
                 why. Run `parcad serve` in a terminal to see it fail on its own."
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    /// A packaged version is what tells an installed copy to update, and
    /// nothing else would notice it lagging a release.
    #[test]
    fn packaging_manifests_carry_the_crate_version() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging");
        for manifest in [
            "plugin/.claude-plugin/plugin.json",
            "plugin/.codex-plugin/plugin.json",
            "mcpb/manifest.json",
            "mcpb/server.json",
        ] {
            let text = std::fs::read_to_string(format!("{root}/{manifest}")).unwrap();
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                json["version"],
                env!("CARGO_PKG_VERSION"),
                "packaging/{manifest} names another version; set it to the workspace's"
            );
        }
        let server = std::fs::read_to_string(format!("{root}/mcpb/server.json")).unwrap();
        let tag = format!("/download/v{}/", env!("CARGO_PKG_VERSION"));
        assert!(
            server.contains(&tag),
            "packaging/mcpb/server.json points at another release's bundle; name v{} and its sha256",
            env!("CARGO_PKG_VERSION")
        );
    }
}
