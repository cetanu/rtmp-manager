use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WebhookEvent {
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl WebhookEvent {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}
