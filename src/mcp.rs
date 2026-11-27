//! Local MCP (Model Context Protocol) server: AI assistants (Claude Code / Claude Desktop, OpenCode,
//! local Ollama models through an MCP client) can read and command the model open in ManageBIM.
//!
//! - "Streamable HTTP" transport (JSON responses only, no SSE) on `127.0.0.1:<port>/mcp`,
//!   protected by a token (`Authorization: Bearer <token>`). Off by default; started from the
//!   "AI connection" window. Connections from other computers are not accepted (loopback only).
//! - `ManageBIM.exe --mcp-stdio`: stdio <-> HTTP bridge for clients that start the server themselves
//!   (Claude Desktop, OpenCode "local"). Reads the port and the token from `%APPDATA%\ManageBIM\mcp.json`,
//!   written by the app when it starts the server.
//!
//! The server thread does not touch the model: each `tools/call` reaches the UI thread (`Call`),
//! where `App::mcp_step` executes it and sends the response back.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub const PROTOCOL: &str = "2025-06-18";
pub const DEFAULT_PORT: u16 = 8765;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    /// the tools that modify the model (properties, visibility, scripts, clash) - allowed by default
    #[serde(default = "yes")]
    pub allow_edit: bool,
}

fn yes() -> bool {
    true
}

impl Default for McpSettings {
    fn default() -> Self {
        McpSettings { enabled: false, port: DEFAULT_PORT, token: String::new(), allow_edit: true }
    }
}

/// Random token (128 bits, hex) - no new dependencies: RandomState is seeded by the system.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut s = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_u32(std::process::id());
        s.push_str(&format!("{:016x}", h.finish()));
    }
    s
}

/// A `tools/call` request sent to the UI thread.
pub struct Call {
    pub tool: String,
    pub args: Value,
    pub reply: Sender<Result<Value, String>>,
}

pub struct Server {
    pub port: u16,
    pub rx: Receiver<Call>,
    stop: Arc<AtomicBool>,
    /// requests received (for the connection window)
    pub requests: Arc<AtomicUsize>,
    pub client: Arc<std::sync::Mutex<String>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(p) = info_path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn info_path() -> Option<std::path::PathBuf> {
    crate::formats::config_root().map(|d| d.join("ManageBIM").join("mcp.json"))
}

/// Starts the server; `wake` wakes the UI when a request arrives (egui idles without input).
pub fn start(port: u16, token: &str, wake: impl Fn() + Send + Sync + 'static) -> Result<Server, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("Port {port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let (tx, rx) = channel::<Call>();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(std::sync::Mutex::new(String::new()));
    let (st, rq, cl) = (stop.clone(), requests.clone(), client.clone());
    let token = token.to_string();
    #[cfg(not(test))]
    let token_file = token.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    std::thread::Builder::new()
        .name("mcp-server".into())
        .spawn(move || {
            while !st.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((s, addr)) => {
                        if !addr.ip().is_loopback() {
                            continue;
                        }
                        let (tx, token, wake, rq, cl) = (tx.clone(), token.clone(), wake.clone(), rq.clone(), cl.clone());
                        std::thread::spawn(move || {
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle_conn(s, &token, &tx, &*wake, &rq, &cl)));
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(40)),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    #[cfg(not(test))]
    if let Some(p) = info_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, json!({ "port": port, "token": token_file, "url": format!("http://127.0.0.1:{port}/mcp") }).to_string());
    }
    Ok(Server { port, rx, stop, requests, client })
}

// ---------------------------------------------------------------- minimal HTTP

struct HttpReq {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpReq {
    fn header(&self, k: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
}

fn read_req(r: &mut BufReader<TcpStream>) -> Option<HttpReq> {
    let mut line = String::new();
    if r.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut it = line.split_whitespace();
    let method = it.next()?.to_string();
    let path = it.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 {
            return None;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
        if headers.len() > 100 {
            return None;
        }
    }
    let len: usize = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    if len > 16 << 20 {
        return None;
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some(HttpReq { method, path, headers, body })
}

fn respond(s: &mut TcpStream, code: u16, reason: &str, ctype: &str, body: &[u8], keep: bool) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: {}\r\n\r\n",
        body.len(),
        if keep { "keep-alive" } else { "close" }
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    s.flush()
}

fn handle_conn(s: TcpStream, token: &str, tx: &Sender<Call>, wake: &dyn Fn(), rq: &AtomicUsize, cl: &std::sync::Mutex<String>) {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(Duration::from_secs(120)));
    let mut w = match s.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut r = BufReader::new(s);
    while let Some(req) = read_req(&mut r) {
        let keep = !req.header("connection").map(|c| c.eq_ignore_ascii_case("close")).unwrap_or(false);
        // DNS rebinding protection: a web page cannot talk to the server
        if let Some(o) = req.header("origin") {
            let ok = o.starts_with("http://127.0.0.1") || o.starts_with("http://localhost") || o.starts_with("http://[::1]");
            if !ok {
                let _ = respond(&mut w, 403, "Forbidden", "text/plain", b"origin not allowed", false);
                return;
            }
        }
        if !req.path.starts_with("/mcp") {
            let _ = respond(&mut w, 404, "Not Found", "text/plain", b"ManageBIM MCP endpoint: /mcp", keep);
            continue;
        }
        let auth = req.header("authorization").unwrap_or("");
        let given = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")).unwrap_or("").trim();
        if token.is_empty() || !ct_eq(given, token) {
            let _ = respond(&mut w, 401, "Unauthorized", "application/json", br#"{"error":"missing or wrong token (Authorization: Bearer ...)"}"#, false);
            return;
        }
        match req.method.as_str() {
            "POST" => {}
            "DELETE" => {
                let _ = respond(&mut w, 200, "OK", "text/plain", b"", keep);
                continue;
            }
            _ => {
                // no SSE stream from the server
                let _ = respond(&mut w, 405, "Method Not Allowed", "text/plain", b"", keep);
                continue;
            }
        }
        rq.fetch_add(1, Ordering::Relaxed);
        let msg: Value = match serde_json::from_slice(&req.body) {
            Ok(v) => v,
            Err(e) => {
                let b = json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}});
                let _ = respond(&mut w, 400, "Bad Request", "application/json", b.to_string().as_bytes(), keep);
                continue;
            }
        };
        let call = |tool: String, args: Value| -> Result<Value, String> {
            let (rtx, rrx) = channel();
            tx.send(Call { tool, args, reply: rtx }).map_err(|_| "ManageBIM is closing".to_string())?;
            wake();
            rrx.recv_timeout(Duration::from_secs(600)).map_err(|_| "ManageBIM did not answer in time".to_string())?
        };
        let out = match &msg {
            Value::Array(items) => {
                let v: Vec<Value> = items.iter().filter_map(|m| dispatch(m, &call, cl)).collect();
                if v.is_empty() {
                    None
                } else {
                    Some(Value::Array(v))
                }
            }
            m => dispatch(m, &call, cl),
        };
        let r = match out {
            Some(v) => respond(&mut w, 200, "OK", "application/json", v.to_string().as_bytes(), keep),
            None => respond(&mut w, 202, "Accepted", "application/json", b"", keep),
        };
        if r.is_err() || !keep {
            return;
        }
    }
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------------------------------------------------------- JSON-RPC / MCP

/// The response to a JSON-RPC message (`None` for notifications).
pub fn dispatch(m: &Value, call: &dyn Fn(String, Value) -> Result<Value, String>, client: &std::sync::Mutex<String>) -> Option<Value> {
    let id = m.get("id").cloned();
    let method = m.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = id?; // notification (notifications/initialized etc.) or a response from the client
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    let res: Result<Value, (i64, String)> = match method {
        "initialize" => {
            if let Some(ci) = params.get("clientInfo") {
                let n = format!("{} {}", ci.get("name").and_then(|v| v.as_str()).unwrap_or("?"), ci.get("version").and_then(|v| v.as_str()).unwrap_or(""));
                if let Ok(mut c) = client.lock() {
                    *c = n.trim().to_string();
                }
            }
            let ver = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOL);
            let ver = if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"].contains(&ver) { ver } else { PROTOCOL };
            Ok(json!({
                "protocolVersion": ver,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "managebim", "title": "ManageBIM", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if !tools().iter().any(|t| t["name"] == name.as_str()) {
                Err((-32602, format!("unknown tool: {name}")))
            } else {
                Ok(match call(name, args) {
                    // ready-made content (the stdio bridge receives it like this from the app)
                    Ok(v) if v.get("__content").is_some() => json!({ "content": v["__content"].clone(), "isError": false }),
                    // image (capture_view): image block + description
                    Ok(v) if v.get("__image").is_some() => json!({
                        "content": [
                            { "type": "image", "data": v["__image"].clone(), "mimeType": v["mime"].as_str().unwrap_or("image/jpeg") },
                            { "type": "text", "text": v["text"].as_str().unwrap_or("") }
                        ],
                        "isError": false
                    }),
                    Ok(v) => {
                        let text = if let Value::String(s) = &v { s.clone() } else { serde_json::to_string_pretty(&v).unwrap_or_default() };
                        json!({ "content": [{ "type": "text", "text": text }], "isError": false })
                    }
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                })
            }
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match res {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((c, e)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": c, "message": e } }),
    })
}

const INSTRUCTIONS: &str = "ManageBIM is a BIM viewer / coordination app. Elements are addressed by id \"model:element\" \
(from find_elements / get_selection). Lengths are in metres. Typical flow: get_model_info -> find_elements -> \
get_properties / select_elements -> set_visibility or set_property; capture_view shows the 3D view as an image. \
Clash tests: list_clash_tests, run_clash_test, get_clash_results, focus_clash + capture_view, update_clash_results, export_clash_bcf. \
run_script executes a Rhai script of the app's Script console for bulk work.";

fn tool(name: &str, title: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": { "type": "object", "properties": props, "required": required },
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false }
    })
}

pub fn tools() -> Vec<Value> {
    let ids = json!({ "type": "array", "items": { "type": "string" }, "description": "Element ids \"model:element\"; omitted = current selection" });
    vec![
        tool("get_model_info", "Model info", "Loaded models (name, file, element / triangle counts, levels, categories), selection and clash-test counts.", json!({}), &[], true),
        tool(
            "find_elements",
            "Find elements",
            "Search elements. All filters are optional, case-insensitive and combined with AND. `text` matches name, category, level, GlobalId or any property value (substring).",
            json!({
                "text": { "type": "string" },
                "category": { "type": "string", "description": "IFC class / category, e.g. IfcWall, Walls, Ducts (substring)" },
                "level": { "type": "string", "description": "Building storey / level (substring)" },
                "property": { "type": "string", "description": "Property name (or group|name) that must exist" },
                "value": { "type": "string", "description": "Value of `property` (substring); with no property, any property value" },
                "visible_only": { "type": "boolean" },
                "limit": { "type": "integer", "description": "Max ids returned (default 100, max 5000)" }
            }),
            &[],
            true,
        ),
        tool("get_properties", "Get properties", "All properties (grouped by property set) of up to 50 elements.", json!({ "ids": ids }), &[], true),
        tool(
            "capture_view",
            "Capture 3D view",
            "Image (JPEG) of the ManageBIM 3D view as the user sees it: selected elements highlighted, hidden ones left out, current visual style and section. Optionally zoom to elements (ids), fit the whole model (fit_all) or use a standard view first.",
            json!({
                "ids": { "type": "array", "items": { "type": "string" }, "description": "Zoom to these elements first" },
                "fit_all": { "type": "boolean", "description": "Zoom to the whole model first" },
                "view": { "type": "string", "enum": ["top", "front", "back", "left", "right", "iso"], "description": "Standard view direction" },
                "width": { "type": "integer", "description": "Pixels (256 - 2048, default 1280)" },
                "height": { "type": "integer", "description": "Pixels (default width × 9/16)" },
                "ray_trace": { "type": "boolean", "description": "Path-traced image (GPU): soft shadows, reflections, sky light; turns the Ray Trace view on" }
            }),
            &[],
            true,
        ),
        tool("get_selection", "Get selection", "Ids and names of the elements selected in ManageBIM.", json!({}), &[], true),
        tool(
            "select_elements",
            "Select elements",
            "Replace the selection in ManageBIM with these elements; optionally zoom the 3D view to them.",
            json!({ "ids": { "type": "array", "items": { "type": "string" } }, "zoom": { "type": "boolean" } }),
            &["ids"],
            false,
        ),
        tool(
            "set_visibility",
            "Set visibility",
            "hide / show / isolate elements (ids or current selection), or show_all to unhide everything.",
            json!({ "action": { "type": "string", "enum": ["hide", "show", "isolate", "show_all"] }, "ids": ids }),
            &["action"],
            false,
        ),
        tool("zoom_to", "Zoom to", "Fit the 3D view to elements (ids / selection) or, with all=true, to the whole model.", json!({ "ids": ids, "all": { "type": "boolean" } }), &[], false),
        tool(
            "set_property",
            "Set property",
            "Write a user property (shown in Properties, exported to IFC / Excel / BCF) on elements (ids or selection). Can be undone in ManageBIM.",
            json!({ "ids": ids, "name": { "type": "string" }, "value": { "type": "string" } }),
            &["name", "value"],
            false,
        ),
        tool("list_clash_tests", "List clash tests", "Clash tests with their settings and result counts by status / severity.", json!({}), &[], true),
        tool(
            "run_clash_test",
            "Run clash test",
            "Run a clash test (by name or index) and return the result summary and the first results.",
            json!({ "test": { "type": "string", "description": "Test name or 0-based index" }, "max_results": { "type": "integer" } }),
            &["test"],
            false,
        ),
        tool(
            "get_clash_results",
            "Get clash results",
            "Results of a clash test with elements, distance, status, severity, assignee and comments; filter by status / severity, page with offset / limit.",
            json!({
                "test": { "type": "string", "description": "Test name or 0-based index (optional when there is one test)" },
                "status": { "type": "string", "enum": ["new", "active", "reviewed", "approved", "resolved"] },
                "severity": { "type": "string", "enum": ["critical", "major", "minor"] },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 50, max 500" }
            }),
            &[],
            true,
        ),
        tool(
            "update_clash_results",
            "Update clash results",
            "Set status (new / active / reviewed / approved / resolved; approved and resolved record who and when), assignee, group, and add a comment, on clash results.",
            json!({
                "test": { "type": "string" },
                "results": { "description": "Array of result indexes or names, or \"all\"" },
                "status": { "type": "string", "enum": ["new", "active", "reviewed", "approved", "resolved"] },
                "assignee": { "type": "string" },
                "comment": { "type": "string", "description": "Added under the existing comments" },
                "group": { "type": "string" }
            }),
            &["results"],
            false,
        ),
        tool(
            "focus_clash",
            "Focus clash",
            "Show clash results in ManageBIM like a click in Clash Detective: the two elements highlighted, the rest dimmed, camera on the clash. Follow with capture_view to see it.",
            json!({ "test": { "type": "string" }, "results": { "description": "Array of result indexes or names" } }),
            &["results"],
            false,
        ),
        tool(
            "export_clash_bcf",
            "Export clashes to BCF",
            "Write the open results of a clash test as a BCF file (viewpoints, elements, status) for Revit, ACC, Solibri, BIMcollab.",
            json!({ "test": { "type": "string" }, "path": { "type": "string", "description": "Full path ending with .bcf or .bcfzip" } }),
            &["path"],
            false,
        ),
        tool(
            "run_script",
            "Run script",
            "Run a Rhai script in the ManageBIM Script console (functions: log, selected_count, find_where_empty, select_where, select_ids, set_prop, create_clash_test, zoom_to_selection). Returns the log lines.",
            json!({ "source": { "type": "string" } }),
            &["source"],
            false,
        ),
        tool("open_model", "Open model", "Open a model file (IFC, RVT export, DWG, NWD, glTF, E57, LAS ...) and add it to the scene. Loading continues in the background.", json!({ "path": { "type": "string" } }), &["path"], false),
    ]
}

/// The tools that change something (blocked when "Allow changes" is off)
pub fn is_edit(tool: &str) -> bool {
    matches!(tool, "set_property" | "run_script" | "open_model" | "run_clash_test" | "update_clash_results" | "export_clash_bcf")
}

// ---------------------------------------------------------------- the stdio bridge

/// `ManageBIM.exe --mcp-stdio`: reads JSON-RPC messages (one per line) from the client and sends them
/// to the server in the app. If the app is not running, it answers initialize / tools/list itself,
/// and the tools return an error explaining what needs to be done.
pub fn stdio_bridge() -> i32 {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let client = std::sync::Mutex::new(String::new());
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(out, "{}", json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}}));
                let _ = out.flush();
                continue;
            }
        };
        let call = |tool: String, args: Value| -> Result<Value, String> { forward_tool(&tool, &args) };
        if let Some(r) = dispatch(&msg, &call, &client) {
            let _ = writeln!(out, "{r}");
            let _ = out.flush();
        }
    }
    0
}

fn forward_tool(tool: &str, args: &Value) -> Result<Value, String> {
    let not_running = "ManageBIM is not running or its MCP server is off. Start ManageBIM and turn on the server in Collaborate › AI connection.";
    let info: Value = info_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).ok_or(not_running)?;
    let port = info["port"].as_u64().ok_or(not_running)? as u16;
    let token = info["token"].as_str().unwrap_or("");
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": args } }).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|_| not_running.to_string())?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(620)));
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&resp);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("bad response from ManageBIM")?;
    if head.contains(" 401 ") {
        return Err("ManageBIM refused the token - restart the server in Collaborate › AI connection.".into());
    }
    let v: Value = serde_json::from_str(body).map_err(|e| format!("bad response from ManageBIM: {e}"))?;
    if let Some(e) = v.get("error") {
        return Err(e["message"].as_str().unwrap_or("error").to_string());
    }
    let r = &v["result"];
    if r["isError"].as_bool().unwrap_or(false) {
        Err(r["content"][0]["text"].as_str().unwrap_or("").to_string())
    } else {
        // all content (text and images) passes through unchanged
        Ok(json!({ "__content": r["content"].clone() }))
    }
}

// ---------------------------------------------------------------- configurations for clients

pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

pub fn cfg_claude_code(port: u16, token: &str) -> String {
    format!("claude mcp add --transport http managebim {} --header \"Authorization: Bearer {token}\"", url(port))
}

pub fn cfg_claude_desktop(exe: &str) -> String {
    serde_json::to_string_pretty(&json!({ "mcpServers": { "managebim": { "command": exe, "args": ["--mcp-stdio"] } } })).unwrap_or_default()
}

pub fn cfg_opencode(port: u16, token: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": { "managebim": { "type": "remote", "url": url(port), "enabled": true, "headers": { "Authorization": format!("Bearer {token}") } } }
    }))
    .unwrap_or_default()
}

pub fn cfg_vscode(port: u16, token: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "servers": { "managebim": { "type": "http", "url": url(port), "headers": { "Authorization": format!("Bearer {token}") } } }
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_roundtrip() {
        let token = new_token();
        assert_eq!(token.len(), 32);
        let srv = start(0, &token, || {}).err();
        // port 0 is refused only by some systems; we use a known free port
        drop(srv);
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let srv = start(port, &token, || {}).expect("start");
        // simulated UI thread
        std::thread::spawn(move || {
            while let Ok(c) = srv.rx.recv() {
                let _ = c.reply.send(Ok(json!({ "tool": c.tool, "args": c.args })));
            }
        });
        let post = |body: &str, tok: &str| -> String {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = format!("POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {tok}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            s.write_all(req.as_bytes()).unwrap();
            let mut r = String::new();
            s.read_to_string(&mut r).unwrap();
            r
        };
        let r = post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"t","version":"1"}}}"#, &token);
        assert!(r.starts_with("HTTP/1.1 200") && r.contains("managebim"), "{r}");
        let r = post(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &token);
        assert!(r.starts_with("HTTP/1.1 202"), "{r}");
        let r = post(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &token);
        assert!(r.contains("find_elements") && r.contains("inputSchema"));
        let r = post(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_selection","arguments":{}}}"#, &token);
        assert!(r.contains("get_selection") && r.contains("\"isError\":false"), "{r}");
        let r = post(r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#, "wrong");
        assert!(r.starts_with("HTTP/1.1 401"), "{r}");
    }
}
