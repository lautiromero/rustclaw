pub mod builder;
pub use builder::build_agent;
pub mod hooks;

// pub type AppAgent = rig::agent::Agent<crate::providers::nvidia::CompletionModel>;
pub type AppAgent =
    rig::agent::Agent<rig::providers::openai::completion::CompletionModel<reqwest::Client>>;
