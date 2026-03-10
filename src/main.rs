mod model;
mod proto;
mod ui;

use crate::model::{TaskDraft, TaskRow, TodoDraft, TodoRow, TodoScope, View};
use crate::proto::{ProtoClient, parse_args};
use crate::ui::run_tui;

pub(crate) struct AppState {
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

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}
