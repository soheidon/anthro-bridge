use std::io::{self, BufRead, Write};

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "success".to_string());
    let auxiliary_path = std::env::args().nth(2);
    if mode == "started-marker" {
        if let Some(path) = &auxiliary_path {
            std::fs::write(path, "started").expect("write process-start marker");
        }
    }
    if mode == "stderr-flood" {
        let _ = io::stderr().write_all(&vec![b'x'; 2 * 1024 * 1024]);
    }
    if mode == "argv-capture" {
        if let Some(path) = &auxiliary_path {
            // Record argv exactly as the OS delivered it, one entry per record
            // separator, so the harness can prove literal (never shell-parsed) arguments.
            let joined = std::env::args()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\u{1f}");
            std::fs::write(path, joined).expect("write argv capture");
        }
    }
    if mode == "env-capture" {
        if let Some(path) = &auxiliary_path {
            // Record all environment variables present in the child process,
            // formatted as KEY=VALUE per line.
            let mut vars: Vec<String> = std::env::vars()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            vars.sort();
            std::fs::write(path, vars.join("\n")).expect("write env capture");
        }
    }
    if mode == "descendant-child" {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
    if mode == "exit-now" {
        // Exits by itself, without ever answering a request.
        std::process::exit(3);
    }
    if mode == "exit-after-delay" {
        // Outlives the graceful shutdown window but exits before the shared
        // cleanup deadline, so the root is only reaped after termination began.
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(3);
    }
    if mode.starts_with("descendant-") {
        if let Some(pid_path) = std::env::args().nth(2) {
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("descendant-child")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn descendant");
            std::fs::write(pid_path, child.id().to_string()).expect("write descendant PID");
            std::mem::forget(child);
        }
    }
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.contains("\"method\":\"notifications/initialized\"") {
            continue;
        }
        let Some(method) = string_field(&line, "method") else {
            continue;
        };
        let id = raw_field(&line, "id").unwrap_or_else(|| "null".to_string());
        let response = match method.as_str() {
            "initialize" => {
                if mode == "hang-initialize" {
                    if let Some(path) = &auxiliary_path {
                        let _ = std::fs::write(path, "ready");
                    }
                    continue;
                }
                response(
                    &id,
                    r#""result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"fake-peer","version":"1"}}}"#,
                )
            }
            "tools/list" => {
                let cursor = string_field(&line, "cursor");
                if mode == "hang-list" || (mode == "paged-hang-list" && cursor.is_some()) {
                    if let Some(path) = &auxiliary_path {
                        let _ = std::fs::write(path, "ready");
                    }
                    continue;
                }
                if mode == "exit-list" || (mode == "paged-exit-list" && cursor.is_some()) {
                    std::process::exit(9);
                }
                let effective_mode = mode.strip_prefix("descendant-").unwrap_or(&mode);
                let mut page_metrics = 0usize;
                if matches!(effective_mode, "page-cap" | "exact-page-cap") {
                    page_metrics = auxiliary_path
                        .as_ref()
                        .and_then(|p| std::fs::read_to_string(p).ok())
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0)
                        + 1;
                    if let Some(path) = &auxiliary_path {
                        let _ = std::fs::write(path, page_metrics.to_string());
                    }
                }
                let (name, next) = match (effective_mode, cursor.as_deref()) {
                    ("paged", None) => ("other", Some("next")),
                    ("paged", Some("next")) => ("plan", None),
                    ("paged-bad-schema", None) => ("other", Some("next")),
                    ("paged-bad-schema", Some("next")) => ("plan", None),
                    ("paged-hang-list" | "paged-exit-list", None) => ("other", Some("next")),
                    ("repeat", None) => ("other", Some("same")),
                    ("repeat", _) => ("plan", Some("same")),
                    ("page-cap", _) => ("other", Some("more")),
                    ("exact-page-cap", _) if page_metrics < 32 => ("other", Some("more")),
                    _ => ("plan", None),
                };
                let next_cursor = if effective_mode == "page-cap"
                    || (effective_mode == "exact-page-cap" && page_metrics < 32)
                {
                    format!(",\"nextCursor\":\"page-{page_metrics}\"")
                } else {
                    next.map(|c| format!(",\"nextCursor\":\"{c}\""))
                        .unwrap_or_default()
                };
                let plan_tool = r#"{"name":"plan","description":"test","inputSchema":{"type":"object","properties":{"schema_version":{"type":"integer"},"role":{"type":"string"},"system_prompt":{"type":"string"},"user_prompt":{"type":"string"},"plan_context":{},"frozen_plan":{}}}}"#;
                let tool = if matches!(effective_mode, "bad-schema" | "paged-bad-schema") {
                    r#"{"name":"plan","inputSchema":{"type":"array"}}"#.to_string()
                } else if effective_mode == "tool-cap" {
                    let mut tools = Vec::with_capacity(257);
                    for index in 0..257 {
                        let tool_name = if index == 256 {
                            "plan".to_string()
                        } else {
                            format!("tool-{index}")
                        };
                        tools.push(format!(
                            r#"{{"name":"{tool_name}","inputSchema":{{"type":"object"}}}}"#
                        ));
                    }
                    format!("{}", tools.join(","))
                } else if effective_mode == "exact-tool-cap" {
                    let mut tools = Vec::with_capacity(256);
                    for index in 0..255 {
                        tools.push(format!(
                            r#"{{"name":"tool-{index}","inputSchema":{{"type":"object"}}}}"#
                        ));
                    }
                    tools.push(plan_tool.to_string());
                    tools.join(",")
                } else if name == "other" {
                    if effective_mode == "exact-page-cap" {
                        format!(r#"{{"name":"tool-{page_metrics}","inputSchema":{{"type":"object"}}}}"#)
                    } else {
                        r#"{"name":"other","inputSchema":{"type":"object"}}"#.to_string()
                    }
                } else {
                    plan_tool.to_string()
                };
                if effective_mode == "tool-cap" {
                    response(&id, &format!(r#""result":{{"tools":[{tool}]}}}}"#))
                } else {
                    response(
                        &id,
                        &format!(r#""result":{{"tools":[{tool}]{next_cursor}}}}}"#),
                    )
                }
            }
            "tools/call" => {
                if matches!(mode.as_str(), "exact-page-cap" | "exact-tool-cap") {
                    std::fs::write(auxiliary_path.as_ref().expect("call marker path"), "called")
                        .expect("write accepted-boundary call marker");
                }
                if mode == "capture-call" {
                    if let Some(path) = &auxiliary_path {
                        // Persist the raw request so the harness can compare the
                        // exact frozen payload that crossed the wire.
                        std::fs::write(path, &line).expect("write tools/call capture");
                    }
                }
                if mode == "started-marker" {
                    if let Some(path) = std::env::args().nth(3) {
                        let _ = std::fs::write(path, "called");
                    }
                }
                if mode == "hang-call" {
                    if let Some(path) = &auxiliary_path {
                        let _ = std::fs::write(path, "ready");
                    }
                    continue;
                }
                if mode == "oversized-frame" {
                    // Emits a protocol frame far past the inbound cap and never
                    // terminates it with a delimiter, so the reader's own bound
                    // is the only thing that can stop it.
                    let mut stdout = io::stdout().lock();
                    let chunk = vec![b'x'; 64 * 1024];
                    for _ in 0..(4 * 1024 * 1024 / chunk.len()) {
                        if stdout.write_all(&chunk).is_err() {
                            break;
                        }
                    }
                    let _ = stdout.flush();
                    continue;
                }
                if matches!(mode.as_str(), "bad-schema" | "tool-cap") {
                    if let Some(path) = &auxiliary_path {
                        let _ = std::fs::write(path, "called");
                    }
                }
                response(
                    &id,
                    r#""result":{"content":[{"type":"text","text":"fake peer success"}],"isError":false}}"#,
                )
            }
            _ => response(
                &id,
                r#""error":{"code":-32601,"message":"unknown method"}}"#,
            ),
        };
        if writeln!(io::stdout().lock(), "{response}").is_err() {
            break;
        }
        let _ = io::stdout().flush();
    }
}

fn response(id: &str, suffix: &str) -> String {
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},{suffix}")
}

fn string_field(line: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn raw_field(line: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let start = line.find(&marker)? + marker.len();
    let rest = line[start..].trim_start();
    let start = line[start..].len() - rest.len() + start;
    let end = line[start..].find([',', '}'])? + start;
    Some(line[start..end].to_string())
}
