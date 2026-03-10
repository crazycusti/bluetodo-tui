use super::model::{TaskDraft, TaskRow, TodoDraft, TodoRow};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpStream;

pub(crate) struct Config {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) token: String,
}

pub(crate) struct ProtoClient {
    token: String,
    next_seq: u64,
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

const TUI_CLIENT_LABEL: &str = "BTTUI";
const SUPPORTED_PROTO_VERSION: i64 = 2;

struct ProtoLine {
    kind: String,
    params: HashMap<String, String>,
    seq: String,
}

pub(crate) fn parse_args() -> Result<Config, String> {
    let mut host = "127.0.0.1".to_string();
    let mut port = 5877;
    let mut token = String::new();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                host = args.next().ok_or("Missing value for --host")?;
            }
            "--port" => {
                let value = args.next().ok_or("Missing value for --port")?;
                port = value.parse::<u16>().map_err(|_| "Invalid --port")?;
            }
            "--token" => {
                token = args.next().ok_or("Missing value for --token")?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }

    Ok(Config { host, port, token })
}

fn print_usage() {
    println!("Usage: bluetodo-tui [--host HOST] [--port PORT] [--token TOKEN]");
}

impl ProtoClient {
    pub(crate) fn connect(config: &Config) -> Result<Self, String> {
        let stream = TcpStream::connect((config.host.as_str(), config.port))
            .map_err(|err| err.to_string())?;
        let _ = stream.set_nodelay(true);
        let reader = BufReader::new(stream.try_clone().map_err(|err| err.to_string())?);
        let writer = BufWriter::new(stream);
        let mut client = ProtoClient {
            token: config.token.clone(),
            next_seq: 1,
            reader,
            writer,
        };
        client.hello()?;
        client.auth()?;
        Ok(client)
    }

    fn hello(&mut self) -> Result<(), String> {
        let lines = self.send_command("HELLO", &[], false)?;
        let ok = lines
            .iter()
            .find(|line| line.kind == "OK")
            .ok_or_else(|| "Missing OK response".to_string())?;
        let proto = ok
            .params
            .get("proto")
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| "Missing proto version".to_string())?;
        if proto != SUPPORTED_PROTO_VERSION {
            return Err(format!(
                "Proto mismatch (server {} vs client {})",
                proto, SUPPORTED_PROTO_VERSION
            ));
        }
        Ok(())
    }

    fn auth(&mut self) -> Result<(), String> {
        let mut params = vec![("client", TUI_CLIENT_LABEL.to_string())];
        if !self.token.is_empty() {
            params.push(("token", self.token.clone()));
        }
        self.send_command("AUTH", &params, false).map(|_| ())
    }

    pub(crate) fn list_todos(&mut self) -> Result<Vec<TodoRow>, String> {
        self.list_todos_with_command("LIST_TODOS")
    }

    pub(crate) fn list_archived_todos(&mut self) -> Result<Vec<TodoRow>, String> {
        self.list_todos_with_command("LIST_ARCHIVED")
    }

    fn list_todos_with_command(&mut self, command: &str) -> Result<Vec<TodoRow>, String> {
        let lines = self.send_command(command, &[], true)?;
        let mut todos = Vec::new();
        for line in lines {
            if line.kind != "TODO" {
                continue;
            }
            let id = parse_i64(&line.params, "id")?;
            let title = line.params.get("title").cloned().unwrap_or_default();
            let description = line.params.get("desc").cloned().unwrap_or_default();
            let order_number = line.params.get("order_number").cloned().unwrap_or_default();
            let purchaser = line.params.get("purchaser").cloned().unwrap_or_default();
            let order_date = line.params.get("order_date").cloned().unwrap_or_default();
            let progress = parse_f64(&line.params, "progress").unwrap_or(0.0);
            let budget_spent = parse_f64(&line.params, "budget_spent").unwrap_or(0.0);
            let budget_planned = parse_f64(&line.params, "budget_planned").unwrap_or(0.0);
            let deadline = line.params.get("deadline").cloned().unwrap_or_default();
            let archived_at = line.params.get("archived_at").cloned().unwrap_or_default();
            todos.push(TodoRow {
                id,
                title,
                description,
                order_number,
                purchaser,
                order_date,
                progress,
                budget_spent,
                budget_planned,
                deadline,
                archived_at,
            });
        }
        Ok(todos)
    }

    pub(crate) fn list_tasks(&mut self, todo_id: i64) -> Result<Vec<TaskRow>, String> {
        let params = [("todo_id", todo_id.to_string())];
        let lines = self.send_command("LIST_TASKS", &params, true)?;
        let mut tasks = Vec::new();
        for line in lines {
            if line.kind != "TASK" {
                continue;
            }
            let id = parse_i64(&line.params, "id")?;
            let title = line.params.get("title").cloned().unwrap_or_default();
            let description = line.params.get("desc").cloned().unwrap_or_default();
            let amount = parse_f64(&line.params, "amount").unwrap_or(0.0);
            let done = parse_i64(&line.params, "done").unwrap_or(0) != 0;
            tasks.push(TaskRow {
                id,
                title,
                description,
                amount,
                done,
            });
        }
        Ok(tasks)
    }

    pub(crate) fn add_todo(&mut self, draft: &TodoDraft) -> Result<i64, String> {
        let mut params = vec![("title", draft.title.clone())];
        if !draft.description.trim().is_empty() {
            params.push(("desc", draft.description.clone()));
        }
        if !draft.order_number.trim().is_empty() {
            params.push(("order_number", draft.order_number.clone()));
        }
        if !draft.purchaser.trim().is_empty() {
            params.push(("purchaser", draft.purchaser.clone()));
        }
        if !draft.order_date.trim().is_empty() {
            params.push(("order_date", draft.order_date.clone()));
        }
        if !draft.budget_spent.trim().is_empty() {
            params.push(("budget_spent", draft.budget_spent.clone()));
        }
        if !draft.budget_planned.trim().is_empty() {
            params.push(("budget_planned", draft.budget_planned.clone()));
        }
        if !draft.deadline.trim().is_empty() {
            params.push(("deadline", draft.deadline.clone()));
        }
        let lines = self.send_command("ADD_TODO", &params, false)?;
        parse_id_from_ok(&lines)
    }

    pub(crate) fn update_todo(&mut self, todo_id: i64, draft: &TodoDraft) -> Result<(), String> {
        let mut params = vec![("id", todo_id.to_string()), ("title", draft.title.clone())];
        if !draft.description.trim().is_empty() {
            params.push(("desc", draft.description.clone()));
        }
        if !draft.order_number.trim().is_empty() {
            params.push(("order_number", draft.order_number.clone()));
        }
        if !draft.purchaser.trim().is_empty() {
            params.push(("purchaser", draft.purchaser.clone()));
        }
        if !draft.order_date.trim().is_empty() {
            params.push(("order_date", draft.order_date.clone()));
        }
        if !draft.budget_spent.trim().is_empty() {
            params.push(("budget_spent", draft.budget_spent.clone()));
        }
        if !draft.budget_planned.trim().is_empty() {
            params.push(("budget_planned", draft.budget_planned.clone()));
        }
        if !draft.deadline.trim().is_empty() {
            params.push(("deadline", draft.deadline.clone()));
        }
        self.send_command("UPDATE_TODO", &params, false).map(|_| ())
    }

    pub(crate) fn add_task(&mut self, todo_id: i64, draft: &TaskDraft) -> Result<i64, String> {
        let mut params = vec![
            ("todo_id", todo_id.to_string()),
            ("title", draft.title.clone()),
        ];
        if !draft.description.trim().is_empty() {
            params.push(("desc", draft.description.clone()));
        }
        if !draft.amount.trim().is_empty() {
            params.push(("amount", draft.amount.clone()));
        }
        let lines = self.send_command("ADD_TASK", &params, false)?;
        parse_id_from_ok(&lines)
    }

    pub(crate) fn update_task(&mut self, task_id: i64, draft: &TaskDraft) -> Result<(), String> {
        let mut params = vec![("id", task_id.to_string()), ("title", draft.title.clone())];
        if !draft.description.trim().is_empty() {
            params.push(("desc", draft.description.clone()));
        }
        if !draft.amount.trim().is_empty() {
            params.push(("amount", draft.amount.clone()));
        }
        self.send_command("UPDATE_TASK", &params, false).map(|_| ())
    }

    pub(crate) fn toggle_task(&mut self, task_id: i64) -> Result<bool, String> {
        let params = [("id", task_id.to_string())];
        let lines = self.send_command("TOGGLE_TASK", &params, false)?;
        let done = lines
            .iter()
            .find(|line| line.kind == "OK")
            .and_then(|line| parse_i64(&line.params, "done").ok())
            .unwrap_or(0);
        Ok(done != 0)
    }

    pub(crate) fn archive_todo(&mut self, todo_id: i64) -> Result<(), String> {
        let params = [("id", todo_id.to_string())];
        self.send_command("ARCHIVE_TODO", &params, false)
            .map(|_| ())
    }

    pub(crate) fn unarchive_todo(&mut self, todo_id: i64) -> Result<(), String> {
        let params = [("id", todo_id.to_string())];
        self.send_command("UNARCHIVE_TODO", &params, false)
            .map(|_| ())
    }

    fn send_command(
        &mut self,
        command: &str,
        params: &[(&str, String)],
        expect_end: bool,
    ) -> Result<Vec<ProtoLine>, String> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut base = String::from(command);
        for (key, value) in params {
            base.push(' ');
            base.push_str(key);
            base.push('=');
            base.push_str(&encode_value(value));
        }
        base.push_str(" seq=");
        base.push_str(&seq.to_string());
        let line = build_proto_line(&base);
        self.writer
            .write_all(line.as_bytes())
            .map_err(|err| err.to_string())?;
        self.writer
            .write_all(b"\r\n")
            .map_err(|err| err.to_string())?;
        self.writer.flush().map_err(|err| err.to_string())?;

        let mut lines = Vec::new();
        loop {
            let mut raw = String::new();
            let bytes = self
                .reader
                .read_line(&mut raw)
                .map_err(|err| err.to_string())?;
            if bytes == 0 {
                return Err("Connection closed".to_string());
            }
            let trimmed = raw.trim_end_matches(&['\r', '\n'][..]);
            if trimmed.is_empty() {
                continue;
            }
            let parsed = parse_proto_line(trimmed)?;
            if parsed.seq != seq.to_string() {
                return Err(format!("SeqMismatch expected={} got={}", seq, parsed.seq));
            }
            if parsed.kind == "ERR" {
                let code = parsed.params.get("code").cloned().unwrap_or_default();
                let msg = parsed.params.get("msg").cloned().unwrap_or_default();
                let label = if code.is_empty() { "ERR" } else { &code };
                return Err(format!("{label}: {msg}"));
            }
            lines.push(parsed);
            if expect_end {
                if lines.last().map(|line| line.kind.as_str()) == Some("END") {
                    break;
                }
            } else if lines.last().map(|line| line.kind.as_str()) == Some("OK") {
                break;
            }
        }
        Ok(lines)
    }
}

fn parse_i64(params: &HashMap<String, String>, key: &str) -> Result<i64, String> {
    params
        .get(key)
        .ok_or_else(|| format!("Missing {key}"))?
        .parse::<i64>()
        .map_err(|_| format!("Bad {key}"))
}

fn parse_f64(params: &HashMap<String, String>, key: &str) -> Result<f64, String> {
    params
        .get(key)
        .ok_or_else(|| format!("Missing {key}"))?
        .parse::<f64>()
        .map_err(|_| format!("Bad {key}"))
}

fn parse_id_from_ok(lines: &[ProtoLine]) -> Result<i64, String> {
    lines
        .iter()
        .find(|line| line.kind == "OK")
        .and_then(|line| parse_i64(&line.params, "id").ok())
        .ok_or_else(|| "Missing id".to_string())
}

fn split_crc(line: &str) -> Option<(String, String)> {
    let marker = " crc32=";
    let idx = line.rfind(marker)?;
    let base = line[..idx].to_string();
    let crc = line[idx + marker.len()..].trim().to_string();
    if crc.is_empty() {
        None
    } else {
        Some((base, crc))
    }
}

fn parse_proto_line(line: &str) -> Result<ProtoLine, String> {
    let (base, crc) = split_crc(line).ok_or_else(|| "MissingCRC".to_string())?;
    let expected = crc32_hex(base.as_bytes());
    if !expected.eq_ignore_ascii_case(&crc) {
        return Err("BadCRC".to_string());
    }
    let mut parts = base.split_whitespace();
    let kind = parts
        .next()
        .ok_or_else(|| "MissingCommand".to_string())?
        .to_string();
    let mut params = HashMap::new();
    for part in parts {
        let (key, value) = part.split_once('=').ok_or_else(|| "BadParam".to_string())?;
        let decoded = decode_value(value)?;
        params.insert(key.to_string(), decoded);
    }
    let seq = params
        .remove("seq")
        .ok_or_else(|| "MissingSeq".to_string())?;
    Ok(ProtoLine { kind, params, seq })
}

fn build_proto_line(base: &str) -> String {
    let crc = crc32_hex(base.as_bytes());
    format!("{base} crc32={crc}")
}

fn encode_value(value: &str) -> String {
    let mut encoded = String::new();
    for &byte in value.as_bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn decode_value(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err("BadEscape".to_string());
                }
                let hi = from_hex(bytes[i + 1]).ok_or_else(|| "BadEscape".to_string())?;
                let lo = from_hex(bytes[i + 2]).ok_or_else(|| "BadEscape".to_string())?;
                output.push((hi << 4) | lo);
                i += 3;
            }
            byte => {
                output.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(output).map_err(|_| "BadUtf8".to_string())
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn crc32_hex(data: &[u8]) -> String {
    format!("{:08X}", crc32(data))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let mut value = (crc ^ byte as u32) & 0xFF;
        for _ in 0..8 {
            if value & 1 != 0 {
                value = 0xEDB88320 ^ (value >> 1);
            } else {
                value >>= 1;
            }
        }
        crc = (crc >> 8) ^ value;
    }
    !crc
}
