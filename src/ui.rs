use super::AppState;
use super::model::{TaskDraft, TaskRow, TodoDraft, TodoRow, TodoScope, View};
use pancurses::{Input, Window, cbreak, curs_set, endwin, initscr, noecho};

pub(crate) fn run_tui(app: &mut AppState) -> Result<(), String> {
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
                if let View::Tasks {
                    todo_id, archived, ..
                } = app.view
                {
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
                TodoScope::Active => {
                    "Enter: tasks  n: neu  e: edit  x: archiv  v: archiv  r: refresh  q: quit"
                }
                TodoScope::Archived => "Enter: tasks  u: restore  v: aktiv  r: refresh  q: quit",
            };
            window.mvaddstr(1, 0, truncate_text(help_line, max_x_usize));
            let list_height = (max_y - (detail_rows + 3)).max(0) as usize;

            app.todo_scroll = adjust_scroll(
                app.todo_selected,
                app.todo_scroll,
                list_height,
                app.todos.len(),
            );

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
                        format!(
                            "{} | {}",
                            todo.order_number,
                            blank_fallback(&todo.purchaser)
                        )
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
        View::Tasks {
            title, archived, ..
        } => {
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
            app.task_scroll = adjust_scroll(
                app.task_selected,
                app.task_scroll,
                list_height,
                app.tasks.len(),
            );

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
    if value.trim().is_empty() { "-" } else { value }
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

fn prompt_todo_form(
    window: &Window,
    initial: Option<&TodoRow>,
) -> Result<Option<TodoDraft>, String> {
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
        "Käufer: ",
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

    let has_order_metadata =
        !order_number.is_empty() || !purchaser.is_empty() || !order_date.is_empty();
    if has_order_metadata
        && (order_number.is_empty() || purchaser.is_empty() || order_date.is_empty())
    {
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

fn prompt_task_form(
    window: &Window,
    initial: Option<&TaskRow>,
) -> Result<Option<TaskDraft>, String> {
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
