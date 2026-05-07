use anyhow::Result;

#[allow(dead_code)] 
pub async fn fetch_document(_url: &str) -> Result<String> {
    // Placeholder: reemplazar por reqwest/katana cuando esté listo
    Ok("Documentación de ejemplo: Este sistema usa RAG con Rig.".to_string())
}
