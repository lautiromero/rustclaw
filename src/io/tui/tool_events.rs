use tokio::sync::mpsc;

/// Eventos de herramientas para notificar progreso en la UI
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// La herramienta comenzó a ejecutarse
    Started {
        /// Nombre de la tool (ej: "read_dir")
        name: String,
        /// Argumentos serializados para mostrar en UI
        args: String,
    },
    /// La herramienta terminó (éxito o fallo)
    Completed { name: String, success: bool },
}

/// Alias para el sender del canal de eventos de tools
pub type ToolEventTx = mpsc::UnboundedSender<ToolEvent>;
/// Alias para el receiver del canal de eventos de tools  
pub type ToolEventRx = mpsc::UnboundedReceiver<ToolEvent>;

/// Helper para crear el canal
pub fn create_channel() -> (ToolEventTx, ToolEventRx) {
    mpsc::unbounded_channel()
}
