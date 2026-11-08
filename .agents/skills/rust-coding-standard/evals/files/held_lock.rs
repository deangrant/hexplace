// Eval fixture for rust-coding-standard case 10. Do not edit.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

pub struct Cache {
    entries: Mutex<Vec<String>>,
}

pub struct Shared {
    pub log: Rc<RefCell<Vec<String>>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    pub async fn refresh(&self, key: &str) {
        let mut guard = self.entries.lock().unwrap();
        guard.push(key.to_owned());
        std::future::ready(()).await;
        guard.push(format!("{key}-done"));
    }
}
