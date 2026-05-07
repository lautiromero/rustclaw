use crate::memory::session_store::DbMessage;
use anyhow::{Result, anyhow};
use rig::completion::Message as RigMessage;

/// Convierte Rig Message → DbMessage (serializando el mensaje completo a JSON)
pub fn rig_to_db(session_id: &str, rig_msg: &RigMessage) -> Result<DbMessage> {
    // Determinar role y serializar el mensaje completo
    let role = match rig_msg {
        RigMessage::System { .. } => "system",
        RigMessage::User { .. } => "user",
        RigMessage::Assistant { .. } => "assistant",
    };

    // Serializar el Message completo (incluye content, id, etc.)
    let content_json = serde_json::to_string(rig_msg)
        .map_err(|e| anyhow!("Failed to serialize message: {}", e))?;

    Ok(DbMessage {
        id: None,
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content_json,
        tool_calls: None, // Podés extraer tool_calls después si es crítico
        timestamp: chrono::Utc::now(),
    })
}

/// Convierte DbMessage → Rig Message (deserializando desde JSON)
pub fn db_to_rig(db_msg: &DbMessage) -> Result<RigMessage> {
    // Deserializar directamente a RigMessage
    let msg: RigMessage = serde_json::from_str(&db_msg.content)
        .map_err(|e| anyhow!("Failed to deserialize message: {}", e))?;

    Ok(msg)
}
