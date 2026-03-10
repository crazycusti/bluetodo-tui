use pancurses::{cbreak, curs_set, endwin, initscr, noecho, Input, Window};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpStream;

struct Config {
    host: String,
    port: u16,
    token: String,
}

struct ProtoClient {
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

#[derive(Clone)]
struct TodoRow {
    id: i64,
    title: String,
    description: String,
    order_number: String,
    purchaser: String,
    order_date: String,
    progress: f64,
    budget_spent: f64,
    budget_planned: f64,
    deadline: String,
    archived_at: String,
}

#[derive(Clone)]
struct TaskRow {
    id: i64,
    title: String,
    description: String,
    amount: f64,
    done: bool,
}

struct TodoDraft {
    title: String,
    description: String,
    order_number: String,
    purchaser: String,
    order_date: String,
    budget_spent: String,
    budget_planned: String,
    deadline: String,
}

struct TaskDraft {
    title: String,
    description: String,
    amount: String,
}

enum TodoScope {
    Active,
    Archived,
}

enum View {
    Todos,
    Tasks {
        todo_id: i64,
        title: String,
        archived: bool,
    },
}

struct AppState {
    client: ProtoClient,
    host: String,
    port: u16,
    todos: Vec<TodoRow>,
    tasks: Vec<TaskRow>,
    view: View,
    scope: TodoScope,
    todo_selected: usize,
    todo_scroll: usize,
    task_selected: usize,
    task_scroll: usize,
    status: String,
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let client = match ProtoClient::connect(&config) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to connect: {err}");
            return;
        }
    };

    let mut app = AppState::new(client, &config.host, config.port);
    app.refresh_todos();

    if let Err(err) = run_tui(&mut app) {
        eprintln!("TUI error: {err}");
    }
}

fn parse_args() -> Result<Config, String> {
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
    fn connect(config: &Config) -> Result<Self, String> {
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

    fn list_todos(&mut self) -> Result<Vec<TodoRow>, String> {
        let lines = self.send_command("LIST_TODOS", &[], true)?;
        let mut todos = Vec::new();
        for line in lines {
            if line.kind != "TODO" {
                continue;
            }
            let id = parse_i64(&line.params, "id")?;
            let title = line.params.get("title").cloned().unwrap_or_default();
            let description = line.params.get("desc").cloned().unwrap_or_default();
            let order_number = line
                .params
                .get("order_number")
                .cloned()
                .unwrap_or_default();
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

    fn list_archived_todos(&mut self) -> Result<Vec<TodoRow>, String> {
        let lines = self.send_command("LIST_ARCHIVED", &[], true)?;
        let mut todos = Vec::new();
        for line in lines {
            if line.kind != "TODO" {
                continue;
            }
            let id = parse_i64(&line.params, "id")?;
            let title = line.params.get("title").cloned().unwrap_or_default();
            let description = line.params.get("desc").cloned().unwrap_or_default();
            let order_number = line
                .params
                .get("order_number")
                .cloned()
                .unwrap_or_default();
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

    fn list_tasks(&mut self, todo_id: i64) -> Result<Vec<TaskRow>, String> {
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

    fn add_todo(
        &mut self,
        draft: &TodoDraft,
    ) -> Result<i64, String> {
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

    fn update_todo(&mut self, todo_id: i64, draft: &TodoDraft) -> Result<(), String> {
        let mut params = vec![
            ("id", todo_id.to_string()),
            ("title", draft.title.clone()),
        ];
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

    fn add_task(&mut self, todo_id: i64, draft: &TaskDraft) -> Result<i64, String> {
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

    fn update_task(&mut self, task_id: i64, draft: &TaskDraft) -> Result<(), String> {
        let mut params = vec![
            ("id", task_id.to_string()),
            ("title", draft.title.clone()),
        ];
        if !draft.description.trim().is_empty() {
            params.push(("desc", draft.description.clone()));
        }
        if !draft.amount.trim().is_empty() {
            params.push(("amount", draft.amount.clone()));
        }
        self.send_command("UPDATE_TASK", &params, false).map(|_| ())
    }

    fn toggle_task(&mut self, task_id: i64) -> Result<bool, String> {
        let params = [("id", task_id.to_string())];
        let lines = self.send_command("TOGGLE_TASK", &params, false)?;
        let done = lines
            .iter()
            .find(|line| line.kind == "OK")
            .and_then(|line| parse_i64(&line.params, "done").ok())
            .unwrap_or(0);
        Ok(done != 0)
    }

    fn archive_todo(&mut self, todo_id: i64) -> Result<(), String> {
        let params = [("id", todo_id.to_string())];
        self.send_command("ARCHIVE_TODO", &params, false).map(|_| ())
    }

    fn unarchive_todo(&mut self, todo_id: i64) -> Result<(), String> {
        let params = [("id", todo_id.to_string())];
        self.send_command("UNARCHIVE_TODO", &params, false).map(|_| ())
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
                return Err(format!(
                    "SeqMismatch expected={} got={}",
                    seq, parsed.seq
                ));
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

impl AppState {
    fn new(client: ProtoClient, host: &str, port: u16) -> Self {
        AppState {
            client,
            host: host.to_string(),
            port,
            todos: Vec::new(),
            tasks: Vec::new(),
            view: View::Todos,
            scope: TodoScope::Active,
            todo_selected: 0,
            todo_scroll: 0,
            task_selected: 0,
            task_scroll: 0,
            status: "Ready".to_string(),
        }
    }

    fn refresh_todos(&mut self) {
        let result = match self.scope {
            TodoScope::Active => self.client.list_todos(),
            TodoScope::Archived => self.client.list_archived_todos(),
        };

        match result {
            Ok(todos) => {
                self.todos = todos;
                self.todo_selected = clamp_index(self.todo_selected, self.todos.len());
                let label = match self.scope {
                    TodoScope::Active => "aktive Todos",
                    TodoScope::Archived => "Archiv-Einträge",
                };
                self.status = format!("Loaded {} {}", self.todos.len(), label);
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn refresh_tasks(&mut self, todo_id: i64) {
        match self.client.list_tasks(todo_id) {
            Ok(tasks) => {
                self.tasks = tasks;
                self.task_selected = clamp_index(self.task_selected, self.tasks.len());
                self.status = format!("Loaded {} tasks", self.tasks.len());
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn open_selected_todo(&mut self) {
        if let Some(todo) = self.todos.get(self.todo_selected) {
            let todo_id = todo.id;
            let title = todo.title.clone();
            let archived = matches!(self.scope, TodoScope::Archived);
            self.view = View::Tasks {
                todo_id,
                title,
                archived,
            };
            self.tasks.clear();
            self.task_selected = 0;
            self.task_scroll = 0;
            self.refresh_tasks(todo_id);
        } else {
            self.status = "No todo selected".to_string();
        }
    }

    fn back_to_todos(&mut self) {
        self.view = View::Todos;
        self.status = "Back to todos".to_string();
    }

    fn toggle_selected_task(&mut self) {
        let task_id = match self.tasks.get(self.task_selected) {
            Some(task) => task.id,
            None => {
                self.status = "No task selected".to_string();
                return;
            }
        };
        match self.client.toggle_task(task_id) {
            Ok(_) => {
                if let View::Tasks { todo_id, .. } = self.view {
                    self.refresh_tasks(todo_id);
                }
                self.status = "Task toggled".to_string();
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn create_todo(&mut self, draft: &TodoDraft) {
        match self.client.add_todo(draft) {
            Ok(id) => {
                self.refresh_todos();
                self.status = format!("Todo gespeichert (id={id})");
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn update_selected_todo(&mut self, todo_id: i64, draft: &TodoDraft) {
        match self.client.update_todo(todo_id, draft) {
            Ok(()) => {
                self.refresh_todos();
                self.status = "Todo aktualisiert".to_string();
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn create_task(&mut self, todo_id: i64, draft: &TaskDraft) {
        match self.client.add_task(todo_id, draft) {
            Ok(id) => {
                self.refresh_tasks(todo_id);
                self.status = format!("Task gespeichert (id={id})");
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn update_selected_task(&mut self, task_id: i64, todo_id: i64, draft: &TaskDraft) {
        match self.client.update_task(task_id, draft) {
            Ok(()) => {
                self.refresh_tasks(todo_id);
                self.status = "Task aktualisiert".to_string();
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn toggle_scope(&mut self) {
        self.scope = match self.scope {
            TodoScope::Active => TodoScope::Archived,
            TodoScope::Archived => TodoScope::Active,
        };
        self.todo_selected = 0;
        self.todo_scroll = 0;
        self.refresh_todos();
    }

    fn archive_selected_todo(&mut self) {
        let todo = match self.todos.get(self.todo_selected) {
            Some(todo) => todo,
            None => {
                self.status = "No todo selected".to_string();
                return;
            }
        };
        let todo_id = todo.id;
        match self.client.archive_todo(todo_id) {
            Ok(_) => {
                self.refresh_todos();
                self.status = "Todo archiviert".to_string();
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn unarchive_selected_todo(&mut self) {
        let todo_id = match self.todos.get(self.todo_selected) {
            Some(todo) => todo.id,
            None => {
                self.status = "No todo selected".to_string();
                return;
            }
        };
        match self.client.unarchive_todo(todo_id) {
            Ok(_) => {
                self.refresh_todos();
                self.status = "Todo wiederhergestellt".to_string();
            }
            Err(err) => {
                self.status = format!("Error: {err}");
            }
        }
    }

    fn selected_todo(&self) -> Option<&TodoRow> {
        self.todos.get(self.todo_selected)
    }

    fn selected_task(&self) -> Option<&TaskRow> {
        self.tasks.get(self.task_selected)
    }
}

fn run_tui(app: &mut AppState) -> Result<(), String> {
    let window = initscr();
    cbreak();
    noecho();
    window.keypad(true);
    let _ = curs_set(0);

    loop {
        draw(&window, app);
        match window.getch() {
            Some(Input::Character('q')) => break,
            Some(Input::Character('r')) => match app.view {
                View::Todos => app.refresh_todos(),
                View::Tasks { todo_id, .. } => app.refresh_tasks(todo_id),
            },
            Some(Input::Character('v')) => {
                if matches!(app.view, View::Todos) {
                    app.toggle_scope();
                }
            }
            Some(Input::Character('n')) => {
                if matches!(app.view, View::Todos) {
                    if matches!(app.scope, TodoScope::Archived) {
                        app.status = "Archiv ist read-only".to_string();
                    } else {
                        match prompt_todo_form(&window, None) {
                            Ok(Some(draft)) => app.create_todo(&draft),
                            Ok(None) => app.status = "Abgebrochen".to_string(),
                            Err(err) => app.status = err,
                        }
                    }
                }
            }
            Some(Input::Character('e')) => match &app.view {
                View::Todos => {
                    if matches!(app.scope, TodoScope::Archived) {
                        app.status = "Archiv ist read-only".to_string();
                    } else if let Some(todo) = app.selected_todo().cloned() {
                        match prompt_todo_form(&window, Some(&todo)) {
                            Ok(Some(draft)) => app.update_selected_todo(todo.id, &draft),
                            Ok(None) => app.status = "Abgebrochen".to_string(),
                            Err(err) => app.status = err,
                        }
                    } else {
                        app.status = "No todo selected".to_string();
                    }
                }
                View::Tasks {
                    todo_id, archived, ..
                } => {
                    if *archived {
                        app.status = "Archiv ist read-only".to_string();
                    } else if let Some(task) = app.selected_task().cloned() {
                        match prompt_task_form(&window, Some(&task)) {
                            Ok(Some(draft)) => app.update_selected_task(task.id, *todo_id, &draft),
                            Ok(None) => app.status = "Abgebrochen".to_string(),
                            Err(err) => app.status = err,
                        }
                    } else {
                        app.status = "No task selected".to_string();
                    }
                }
            },
            Some(Input::Character('b')) | Some(Input::KeyLeft) => {
                if matches!(app.view, View::Tasks { .. }) {
                    app.back_to_todos();
                }
            }
            Some(Input::Character('a')) => {
                if let View::Tasks { todo_id, archived, .. } = app.view {
                    if archived {
                        app.status = "Archiv ist read-only".to_string();
                    } else {
                        match prompt_task_form(&window, None) {
                            Ok(Some(draft)) => app.create_task(todo_id, &draft),
                            Ok(None) => app.status = "Abgebrochen".to_string(),
                            Err(err) => app.status = err,
                        }
                    }
                }
            }
            Some(Input::Character('t')) => {
                if let View::Tasks { archived, .. } = app.view {
                    if archived {
                        app.status = "Archiv ist read-only".to_string();
                    } else {
                        app.toggle_selected_task();
                    }
                }
            }
            Some(Input::Character('x')) => {
                if matches!(app.view, View::Todos) && matches!(app.scope, TodoScope::Active) {
                    app.archive_selected_todo();
                }
            }
            Some(Input::Character('u')) => {
                if matches!(app.view, View::Todos) && matches!(app.scope, TodoScope::Archived) {
                    app.unarchive_selected_todo();
                }
            }
            Some(Input::KeyUp) => match app.view {
                View::Todos => {
                    if app.todo_selected > 0 {
                        app.todo_selected -= 1;
                    }
                }
                View::Tasks { .. } => {
                    if app.task_selected > 0 {
                        app.task_selected -= 1;
                    }
                }
            },
            Some(Input::KeyDown) => match app.view {
                View::Todos => {
                    if app.todo_selected + 1 < app.todos.len() {
                        app.todo_selected += 1;
                    }
                }
                View::Tasks { .. } => {
                    if app.task_selected + 1 < app.tasks.len() {
                        app.task_selected += 1;
                    }
                }
            },
            Some(Input::Character('\n')) | Some(Input::KeyEnter) | Some(Input::KeyRight) => {
                if matches!(app.view, View::Todos) {
                    app.open_selected_todo();
                }
            }
            _ => {}
        }
    }

    endwin();
    Ok(())
}

fn draw(window: &Window, app: &mut AppState) {
    window.erase();
    let (max_y, max_x) = window.get_max_yx();
    if max_y < 3 || max_x < 10 {
        window.refresh();
        return;
    }
    let max_x_usize = max_x.max(0) as usize;
    let detail_rows = 3;

    match &app.view {
        View::Todos => {
            let scope_label = match app.scope {
                TodoScope::Active => "Aktiv",
                TodoScope::Archived => "Archiv",
            };
            let header = format!(
                "BlueTodo TUI {}:{}  {}: {}",
                app.host,
                app.port,
                scope_label,
                app.todos.len()
            );
            window.mvaddstr(0, 0, truncate_text(&header, max_x_usize));
            let help_line = match app.scope {
                TodoScope::Active => "Enter: tasks  n: neu  e: edit  x: archiv  v: archiv  r: refresh  q: quit",
                TodoScope::Archived => "Enter: tasks  u: restore  v: aktiv  r: refresh  q: quit",
            };
            window.mvaddstr(1, 0, truncate_text(help_line, max_x_usize));
            let list_height = (max_y - (detail_rows + 3)).max(0) as usize;

            app.todo_scroll =
                adjust_scroll(app.todo_selected, app.todo_scroll, list_height, app.todos.len());

            if app.todos.is_empty() {
                window.mvaddstr(2, 0, "No todos. Press r to refresh.");
            } else {
                for row in 0..list_height {
                    let idx = app.todo_scroll + row;
                    if idx >= app.todos.len() {
                        break;
                    }
                    let todo = &app.todos[idx];
                    let marker = if idx == app.todo_selected { ">" } else { " " };
                    let deadline = if todo.deadline.is_empty() {
                        "-".to_string()
                    } else {
                        todo.deadline.clone()
                    };
                    let meta = if !todo.order_number.is_empty() {
                        format!("{} | {}", todo.order_number, blank_fallback(&todo.purchaser))
                    } else if !todo.description.is_empty() {
                        todo.description.clone()
                    } else {
                        "-".to_string()
                    };
                    let mut line = format!(
                        "{marker} {} | {} | {}% | {:.2}/{:.2} | {}",
                        todo.title,
                        meta,
                        todo.progress.round() as i64,
                        todo.budget_spent,
                        todo.budget_planned,
                        deadline
                    );
                    if matches!(app.scope, TodoScope::Archived) && !todo.archived_at.is_empty() {
                        line.push_str(" | archiv ");
                        line.push_str(&todo.archived_at);
                    }
                    let line = truncate_text(&line, max_x_usize);
                    window.mvaddstr((2 + row) as i32, 0, line);
                }
            }

            if let Some(todo) = app.selected_todo() {
                let detail_y = max_y - 4;
                let order_line = if !todo.order_number.is_empty()
                    || !todo.purchaser.is_empty()
                    || !todo.order_date.is_empty()
                {
                    format!(
                        "Order: {} | {} | {}",
                        blank_fallback(&todo.order_number),
                        blank_fallback(&todo.purchaser),
                        blank_fallback(&todo.order_date)
                    )
                } else {
                    format!("Desc: {}", blank_fallback(&todo.description))
                };
                let timing_line = format!(
                    "Deadline: {} | Archiv: {}",
                    blank_fallback(&todo.deadline),
                    blank_fallback(&todo.archived_at)
                );
                let budget_line = format!(
                    "Budget: {:.2}/{:.2} | Progress: {:.0}%",
                    todo.budget_spent, todo.budget_planned, todo.progress
                );
                window.mvaddstr(detail_y, 0, truncate_text(&order_line, max_x_usize));
                window.mvaddstr(detail_y + 1, 0, truncate_text(&timing_line, max_x_usize));
                window.mvaddstr(detail_y + 2, 0, truncate_text(&budget_line, max_x_usize));
            }
        }
        View::Tasks { title, archived, .. } => {
            let header = format!(
                "BlueTodo TUI {}:{}  Tasks: {}",
                app.host,
                app.port,
                app.tasks.len()
            );
            window.mvaddstr(0, 0, truncate_text(&header, max_x_usize));
            let title_line = format!("Todo: {}", title);
            window.mvaddstr(1, 0, truncate_text(&title_line, max_x_usize));
            let help_line = if *archived {
                "Archiv (read-only)  b: back  r: refresh  q: quit"
            } else {
                "a: add  e: edit  t: toggle  b: back  r: refresh  q: quit"
            };
            window.mvaddstr(2, 0, truncate_text(help_line, max_x_usize));

            let list_height = (max_y - (detail_rows + 4)).max(0) as usize;
            app.task_scroll =
                adjust_scroll(app.task_selected, app.task_scroll, list_height, app.tasks.len());

            if app.tasks.is_empty() {
                window.mvaddstr(3, 0, "No tasks. Press r to refresh.");
            } else {
                for row in 0..list_height {
                    let idx = app.task_scroll + row;
                    if idx >= app.tasks.len() {
                        break;
                    }
                    let task = &app.tasks[idx];
                    let marker = if idx == app.task_selected { ">" } else { " " };
                    let done = if task.done { "[x]" } else { "[ ]" };
                    let mut line = format!("{marker} {done} {}", task.title);
                    if task.amount != 0.0 {
                        line.push_str(&format!(" | {:.2}", task.amount));
                    }
                    if !task.description.is_empty() {
                        line.push_str(" - ");
                        line.push_str(&task.description);
                    }
                    let line = truncate_text(&line, max_x_usize);
                    window.mvaddstr((3 + row) as i32, 0, line);
                }
            }

            if let Some(task) = app.selected_task() {
                let detail_y = max_y - 4;
                let state_line = format!(
                    "Task: {} | Status: {}",
                    task.title,
                    if task.done { "done" } else { "open" }
                );
                let desc_line = format!("Desc: {}", blank_fallback(&task.description));
                let amount_line = format!("Amount: {:.2}", task.amount);
                window.mvaddstr(detail_y, 0, truncate_text(&state_line, max_x_usize));
                window.mvaddstr(detail_y + 1, 0, truncate_text(&desc_line, max_x_usize));
                window.mvaddstr(detail_y + 2, 0, truncate_text(&amount_line, max_x_usize));
            }
        }
    }

    let status = truncate_text(&app.status, max_x_usize);
    window.mvaddstr(max_y - 1, 0, status);
    window.refresh();
}

fn adjust_scroll(selected: usize, scroll: usize, height: usize, len: usize) -> usize {
    if len == 0 || height == 0 {
        return 0;
    }
    let mut new_scroll = scroll.min(len.saturating_sub(1));
    if selected < new_scroll {
        new_scroll = selected;
    } else if selected >= new_scroll + height {
        new_scroll = selected + 1 - height;
    }
    if new_scroll + height > len {
        new_scroll = len.saturating_sub(height);
    }
    new_scroll
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        index.min(len - 1)
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

fn truncate_text(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    if max <= 3 {
        return value.chars().take(max).collect();
    }
    let mut output: String = value.chars().take(max - 3).collect();
    output.push_str("...");
    output
}

fn blank_fallback(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value
    }
}

fn prompt_input(window: &Window, prompt: &str, initial: &str, allow_empty: bool) -> Option<String> {
    let (max_y, max_x) = window.get_max_yx();
    let line_y = max_y.saturating_sub(1);
    let max_x_usize = max_x.max(0) as usize;
    let mut input = initial.to_string();
    let _ = curs_set(1);
    loop {
        let line = format!("{prompt}{input}");
        window.mvaddstr(line_y, 0, truncate_text(&line, max_x_usize));
        window.clrtoeol();
        window.refresh();
        match window.getch() {
            Some(Input::Character('\n')) | Some(Input::KeyEnter) => break,
            Some(Input::Character('\u{1b}')) => {
                let _ = curs_set(0);
                return None;
            }
            Some(Input::KeyBackspace)
            | Some(Input::Character('\u{8}'))
            | Some(Input::Character('\u{7f}')) => {
                input.pop();
            }
            Some(Input::Character(c)) if !c.is_control() => {
                input.push(c);
            }
            _ => {}
        }
    }
    let _ = curs_set(0);
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() && !allow_empty {
        None
    } else {
        Some(trimmed)
    }
}

fn prompt_todo_form(window: &Window, initial: Option<&TodoRow>) -> Result<Option<TodoDraft>, String> {
    let title = match prompt_input(
        window,
        "Titel: ",
        initial.map(|todo| todo.title.as_str()).unwrap_or(""),
        false,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let description = match prompt_input(
        window,
        "Beschreibung: ",
        initial.map(|todo| todo.description.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let order_number = match prompt_input(
        window,
        "Bestellnummer: ",
        initial.map(|todo| todo.order_number.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let purchaser = match prompt_input(
        window,
        "Kaeufer: ",
        initial.map(|todo| todo.purchaser.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let order_date = match prompt_input(
        window,
        "Bestelldatum YYYY-MM-DD: ",
        initial.map(|todo| todo.order_date.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let budget_spent = match prompt_input(
        window,
        "Budget Ist: ",
        &initial
            .map(|todo| format!("{:.2}", todo.budget_spent))
            .unwrap_or_default(),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let budget_planned = match prompt_input(
        window,
        "Budget Plan: ",
        &initial
            .map(|todo| format!("{:.2}", todo.budget_planned))
            .unwrap_or_default(),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let deadline = match prompt_input(
        window,
        "Deadline YYYY-MM-DD: ",
        initial.map(|todo| todo.deadline.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };

    let has_order_metadata = !order_number.is_empty() || !purchaser.is_empty() || !order_date.is_empty();
    if has_order_metadata && (order_number.is_empty() || purchaser.is_empty() || order_date.is_empty()) {
        return Err("Order-Metadaten nur komplett".to_string());
    }

    Ok(Some(TodoDraft {
        title,
        description,
        order_number,
        purchaser,
        order_date,
        budget_spent,
        budget_planned,
        deadline,
    }))
}

fn prompt_task_form(window: &Window, initial: Option<&TaskRow>) -> Result<Option<TaskDraft>, String> {
    let title = match prompt_input(
        window,
        "Task-Titel: ",
        initial.map(|task| task.title.as_str()).unwrap_or(""),
        false,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let description = match prompt_input(
        window,
        "Beschreibung: ",
        initial.map(|task| task.description.as_str()).unwrap_or(""),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };
    let amount = match prompt_input(
        window,
        "Betrag: ",
        &initial
            .map(|task| format!("{:.2}", task.amount))
            .unwrap_or_default(),
        true,
    ) {
        Some(value) => value,
        None => return Ok(None),
    };

    Ok(Some(TaskDraft {
        title,
        description,
        amount,
    }))
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
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| "BadParam".to_string())?;
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
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
        {
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
