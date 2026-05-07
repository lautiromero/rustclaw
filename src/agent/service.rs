use anyhow::Result;
use crate::agent::AppAgent;

pub struct AgentService {
    agent: AppAgent,
}

impl AgentService {
    pub fn new(agent: AppAgent) -> Self { Self { agent } }

    pub async fn chat(&self, query: &str) -> Result<String> {
        let response = self.agent.prompt(query).send().await?;
        Ok(response)
    }
}
