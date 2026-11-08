// Eval fixture for rust-coding-standard case 5. Do not edit.

use std::sync::Mutex;

pub struct Worker {
    name: String,
    cache: Mutex<String>,
}

impl Worker {
    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn greet(label: &String) -> String {
        format!("hello {label}")
    }

    pub fn report(verbose: bool) -> String {
        if verbose {
            "detailed".to_owned()
        } else {
            "brief".to_owned()
        }
    }

    pub fn parse_count(raw: &str) -> Result<u32, String> {
        Ok(raw.parse::<u32>().unwrap())
    }

    pub fn cached_len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    #[allow(clippy::too_many_lines)]
    pub fn run(&self, input: &str) -> Result<String, String> {
        println!("running with {input}");
        let owned = input.to_owned();
        let copy = owned.clone();
        let _ = Self::greet(&copy);
        Self::parse_count(input)?;
        Ok(self.get_name().to_owned())
    }
}
