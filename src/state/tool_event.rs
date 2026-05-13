/// Events emitted by tools to report their execution status to the UI.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// A tool has started executing.
    Started {
        name: String,
        args: String,
    },
    /// A tool has completed execution.
    Completed {
        name: String,
    },
}