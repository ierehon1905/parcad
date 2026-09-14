//! The running host's tools, from the shell.
//!
//! `parcad tools` and `parcad call` are an MCP client for the host that
//! `parcad serve` or the desktop app runs, so whatever a model can do over
//! `/mcp` a shell can do too — the same names, the same arguments, the same
//! reply. Parity is by construction: there is no second list of tools to keep
//! in step, and a tool added to `mcp.rs` is on the command line the same day.
//!
//! Streamable HTTP over a loopback socket, written out by hand. The exchange is
//! three POSTs of JSON, and a client library would bring an HTTP stack the CLI
//! has no other use for. The host answers a POST with plain JSON when it can
//! (`json_response` in `mcp.rs`) and falls back to an event stream when it
//! cannot, so both are read.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

/// `parcad tools [--json]`: what the host offers, as a person or a shell
/// script would read it. `--json` is the tool list as the protocol sends it,
/// schemas included.
pub fn tools(args: impl Iterator<Item = String>) -> Result<()> {
    let mut raw = false;
    for arg in args {
        match arg.as_str() {
            "--json" => raw = true,
            other => bail!("unknown argument {other:?}; usage: parcad tools [--json]"),
        }
    }
    let mut client = Client::connect(parcad_host::http::port())?;
    let listed = client.request("tools/list", json!({}))?;
    if raw {
        println!("{}", serde_json::to_string_pretty(&listed)?);
        return Ok(());
    }
    let tools = listed["tools"].as_array().cloned().unwrap_or_default();
    for tool in &tools {
        let name = tool["name"].as_str().unwrap_or("?");
        println!("{name}");
        println!(
            "    {}",
            first_sentence(tool["description"].as_str().unwrap_or(""))
        );
        let schema = &tool["inputSchema"];
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(properties) = schema["properties"].as_object() {
            for (key, property) in properties {
                let kind = type_name(property);
                let need = if required.contains(&key.as_str()) {
                    ", required"
                } else {
                    ""
                };
                let about = first_sentence(property["description"].as_str().unwrap_or(""));
                if about.is_empty() {
                    println!("    --set {key}  ({kind}{need})");
                } else {
                    println!("    --set {key}  ({kind}{need}): {about}");
                }
            }
        }
        println!();
    }
    println!(
        "{} tools. `parcad call <tool> --set key=value --set key=@file --set key:=<json>`; \
         `parcad tools --json` for the full schemas.",
        tools.len()
    );
    Ok(())
}

/// `parcad call <tool> [JSON | @file | -] [--set ...] [--out DIR]`.
///
/// Arguments are one JSON object, given inline, from a file, or from stdin,
/// and then `--set` pairs laid over it: `key=value` a string, `key=@file` the
/// file's contents as a string (which is how a script travels without shell
/// quoting), `key:=<json>` anything else. Images in the reply are written to
/// `--out` and named on stderr; everything else goes to stdout.
pub fn call(args: impl Iterator<Item = String>) -> Result<()> {
    let mut args = args.peekable();
    let tool = match args.next() {
        Some(name) if !name.starts_with('-') => name,
        _ => bail!("usage: parcad call <tool> [JSON | @file | -] [--set key=value]... [--out DIR]"),
    };
    let mut arguments = serde_json::Map::new();
    let mut out = PathBuf::from("out");
    if let Some(first) = args.peek() {
        if !first.starts_with("--") {
            let source = args.next().unwrap();
            let text = match source.as_str() {
                "-" => {
                    let mut text = String::new();
                    std::io::stdin().read_to_string(&mut text)?;
                    text
                }
                file if file.starts_with('@') => std::fs::read_to_string(&file[1..])
                    .with_context(|| format!("reading {}", &file[1..]))?,
                inline => inline.to_string(),
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => arguments = map,
                Ok(_) => bail!("the arguments must be a JSON object"),
                Err(e) => bail!(
                    "the arguments are not JSON ({e}); a script goes in with --set script=@part.js"
                ),
            }
        }
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--set" | "-s" => {
                let pair = args
                    .next()
                    .context("--set needs key=value, key=@file or key:=json")?;
                let (key, value) = parse_set(&pair)?;
                arguments.insert(key, value);
            }
            "--out" => out = args.next().context("--out needs a directory")?.into(),
            other => bail!("unknown argument {other:?}"),
        }
    }

    let mut client = Client::connect(parcad_host::http::port())?;
    let result = client.request(
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    )?;

    let failed = result["isError"].as_bool().unwrap_or(false);
    let mut texts = Vec::new();
    let mut images = 0;
    for item in result["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("text") => texts.push(item["text"].as_str().unwrap_or("").to_string()),
            Some("image") => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(item["data"].as_str().unwrap_or(""))
                    .context("decoding an image the host sent")?;
                let extension = match item["mimeType"].as_str() {
                    Some("image/jpeg") => "jpg",
                    _ => "png",
                };
                std::fs::create_dir_all(&out)?;
                let path = out.join(format!("{tool}-{images}.{extension}"));
                std::fs::write(&path, bytes)?;
                eprintln!("wrote {}", path.display());
                images += 1;
            }
            _ => {}
        }
    }
    if failed {
        for text in &texts {
            eprintln!("{text}");
        }
        std::process::exit(2);
    }
    // A tool that returns structured content also puts the same JSON in a text
    // item; print it once, formatted.
    if let Some(structured) = result.get("structuredContent") {
        println!("{}", serde_json::to_string_pretty(structured)?);
    } else {
        for text in &texts {
            println!("{text}");
        }
    }
    Ok(())
}

fn parse_set(pair: &str) -> Result<(String, Value)> {
    if let Some((key, raw)) = pair.split_once(":=") {
        let value = serde_json::from_str(raw)
            .with_context(|| format!("{key}:= needs JSON, got {raw:?}"))?;
        return Ok((key.to_string(), value));
    }
    let (key, raw) = pair
        .split_once('=')
        .with_context(|| format!("--set needs key=value, got {pair:?}"))?;
    let value = match raw.strip_prefix('@') {
        Some(file) => std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?,
        None => raw.to_string(),
    };
    Ok((key.to_string(), Value::String(value)))
}

fn type_name(property: &Value) -> String {
    match &property["type"] {
        Value::String(t) => t.clone(),
        Value::Array(ts) => ts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ if property.get("enum").is_some() => "enum".into(),
        _ => "object".into(),
    }
}

/// The first sentence of a description, which is written to stand alone; the
/// paragraph after it is for a model with a context window, not a listing.
fn first_sentence(text: &str) -> String {
    let paragraph = text.split("\n\n").next().unwrap_or("").replace('\n', " ");
    match paragraph.find(". ") {
        Some(end) => paragraph[..=end].to_string(),
        None => paragraph,
    }
}

// ---------------------------------------------------------------- the client

/// The version the host speaks and Claude Code speaks; older ones keep a
/// session and different headers, and the client speaks only this one.
const PROTOCOL: &str = "2026-07-28";

struct Client {
    port: u16,
    session: Option<String>,
    next_id: u64,
}

impl Client {
    fn connect(port: u16) -> Result<Self> {
        let mut client = Client {
            port,
            session: None,
            next_id: 1,
        };
        client.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "parcad-cli", "version": env!("CARGO_PKG_VERSION") },
            }),
        )?;
        client.post(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        Ok(client)
    }

    /// One JSON-RPC request; the `result`, or the server's error as ours.
    fn request(&mut self, method: &str, mut params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        // Protocol 2026-07-28 has no session (SEP-2567): what `initialize`
        // used to establish, every request now carries in its `_meta`.
        if method != "initialize" {
            params["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": PROTOCOL,
                "io.modelcontextprotocol/clientCapabilities": {},
            });
        }
        let (_, body) = self.post(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }))?;
        let message = body.context("the host sent an empty reply")?;
        if let Some(error) = message.get("error") {
            bail!(
                "{}: {}",
                method,
                error["message"]
                    .as_str()
                    .unwrap_or("the host refused the request")
            );
        }
        Ok(message["result"].clone())
    }

    /// One POST to `/mcp`. Returns the response's content type and the
    /// JSON-RPC message in it, which is absent for an accepted notification.
    fn post(&mut self, body: &Value) -> Result<(String, Option<Value>)> {
        let address = format!("127.0.0.1:{}", self.port);
        let mut stream = TcpStream::connect(&address).with_context(|| {
            format!(
                "no parcad is listening on {address}. Start one — `parcad serve`, \
                 `brew services start parcad`, or the desktop app — or name its port \
                 with PARCAD_HTTP_PORT."
            )
        })?;
        let payload = serde_json::to_vec(body)?;
        let mut request = format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
             MCP-Protocol-Version: {PROTOCOL}\r\nConnection: close\r\n",
            payload.len()
        );
        if let Some(session) = &self.session {
            request.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
        }
        // SEP-2243: from protocol 2026-07-28 the method, and for a tool call
        // its name, are repeated in headers so a proxy can route without
        // reading the body. The host refuses a request without them.
        if let Some(method) = body["method"].as_str() {
            request.push_str(&format!("Mcp-Method: {method}\r\n"));
            if method == "tools/call" {
                if let Some(name) = body["params"]["name"].as_str() {
                    request.push_str(&format!("Mcp-Name: {name}\r\n"));
                }
            }
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes())?;
        stream.write_all(&payload)?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        let (head, body) = split_response(&raw)?;
        let status = head.lines().next().unwrap_or("");
        let code: u16 = status
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let header = |name: &str| -> Option<String> {
            head.lines().skip(1).find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_string())
            })
        };
        if let Some(session) = header("mcp-session-id") {
            self.session = Some(session);
        }
        let body = if header("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
            dechunk(body)?
        } else {
            body.to_vec()
        };
        let text = String::from_utf8_lossy(&body).into_owned();
        if !(200..300).contains(&code) {
            // A refusal is a JSON-RPC error in the body, written for whoever
            // caused it; pass its message through whole rather than the wrapper.
            if let Some(message) = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|m| m["error"]["message"].as_str().map(str::to_string))
            {
                bail!("{message}");
            }
            bail!("the host answered {status}: {}", text.trim());
        }
        let content_type = header("content-type").unwrap_or_default();
        let message = if content_type.starts_with("text/event-stream") {
            // The last message on the stream is the reply; the ones before it
            // are notifications the CLI has no use for.
            text.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
                .filter(|message| message.get("id").is_some())
                .next_back()
        } else if text.trim().is_empty() {
            None
        } else {
            Some(serde_json::from_str(&text).context("the host's reply was not JSON")?)
        };
        Ok((content_type, message))
    }
}

fn split_response(raw: &[u8]) -> Result<(String, &[u8])> {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("the host's reply had no headers")?;
    Ok((
        String::from_utf8_lossy(&raw[..end]).into_owned(),
        &raw[end + 4..],
    ))
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .context("a chunked reply ended mid-chunk")?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .context("a chunk size was not hexadecimal")?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        out.extend_from_slice(body.get(..size).context("a chunk was cut short")?);
        body = body.get(size + 2..).unwrap_or(&[]);
    }
}
