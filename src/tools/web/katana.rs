use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum KatanaError {
    #[error("Exec failed: {0}")]
    Exec(String),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct KatanaTool;
impl KatanaTool { pub fn new() -> Self { Self } }

impl Tool for KatanaTool {
    const NAME: &'static str = "katana_crawl";
    type Error = KatanaError;
    type Args = KatanaArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            description: "Crawlea una URL con katana".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL a crawlear" }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(format!("✅ Katana: {}", args.url))
    }
}

#[derive(Deserialize, Serialize)]
pub struct KatanaArgs { pub url: String }
