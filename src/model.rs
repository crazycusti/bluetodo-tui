#[derive(Clone)]
pub(crate) struct TodoRow {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) order_number: String,
    pub(crate) purchaser: String,
    pub(crate) order_date: String,
    pub(crate) progress: f64,
    pub(crate) budget_spent: f64,
    pub(crate) budget_planned: f64,
    pub(crate) deadline: String,
    pub(crate) archived_at: String,
}

#[derive(Clone)]
pub(crate) struct TaskRow {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) amount: f64,
    pub(crate) done: bool,
}

pub(crate) struct TodoDraft {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) order_number: String,
    pub(crate) purchaser: String,
    pub(crate) order_date: String,
    pub(crate) budget_spent: String,
    pub(crate) budget_planned: String,
    pub(crate) deadline: String,
}

pub(crate) struct TaskDraft {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) amount: String,
}

pub(crate) enum TodoScope {
    Active,
    Archived,
}

pub(crate) enum View {
    Todos,
    Tasks {
        todo_id: i64,
        title: String,
        archived: bool,
    },
}
