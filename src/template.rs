use crate::gotify::GotifyMessage;
use log::warn;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct TemplateEngine {
    default_path: PathBuf,
}

impl TemplateEngine {
    pub fn new(default: PathBuf) -> Self {
        Self { default_path: default }
    }

    pub fn render(&self, msg: &GotifyMessage, override_path: Option<&Path>) -> String {
        let path = override_path.unwrap_or(self.default_path.as_path());
        let template = fs::read_to_string(path).unwrap_or_else(|err| {
            warn!("Could not read template {}: {}", path.display(), err);
            default_template()
        });

        template
            .replace("[TITLE]", msg.title.as_deref().unwrap_or("(no title)"))
            .replace("[MESSAGE]", msg.message.as_deref().unwrap_or(""))
            .replace("[APP_ID]", &msg.app_id.to_string())
            .replace("[MESSAGE_ID]", &msg.id.to_string())
            .replace("[PRIORITY]", &msg.priority.to_string())
    }
}

pub fn default_template() -> String {
    "### [TITLE]\n\n[MESSAGE]\n\nApp: [APP_ID] • Priority: [PRIORITY] • Message ID: [MESSAGE_ID]\n".to_string()
}
