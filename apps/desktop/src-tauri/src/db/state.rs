use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub struct AppDbState {
    pub ui_conn: Arc<Mutex<Connection>>,
    pub worker_conn: Arc<Mutex<Connection>>,
}

impl AppDbState {
    pub fn new(ui_conn: Connection, worker_conn: Connection) -> Self {
        Self {
            ui_conn: Arc::new(Mutex::new(ui_conn)),
            worker_conn: Arc::new(Mutex::new(worker_conn)),
        }
    }

    pub fn worker_conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.worker_conn)
    }
}
