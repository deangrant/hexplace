// Eval fixture for rust-coding-standard case 11. Do not edit.

pub struct Summary {
    label: String,
}

impl Summary {
    pub fn get_label(&self) -> &String {
        &self.label
    }
}

pub fn summarize(names: &Vec<String>) -> Summary {
    if names.is_empty() {
        return Summary {
            label: "unknown".to_string(),
        };
    }
    let mut label = String::new();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            label = label + ", ";
        }
        label = label + n;
    }
    Summary { label }
}
