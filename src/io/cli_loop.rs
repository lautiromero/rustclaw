use crate::memory::session_store::SessionStore;
use anyhow::Result;
use rig::completion::{Chat, Message as RigMessage};
use std::io;
use std::io::Write;
use std::sync::Arc;

use crate::agent::AppAgent;
use crate::context::injector::ContextInjector;

pub async fn run_cli<M, S>(
    agent: AppAgent,
    context_injector: Option<ContextInjector<M>>,
    session_store: Option<Arc<S>>,
    session_id: String,
) -> Result<()>
where
    M: rig::embeddings::EmbeddingModel + Clone + Send + Sync + 'static,
    S: SessionStore + 'static,
{
    let mut chat_history: Vec<RigMessage> = Vec::new();

    // Cargar historial desde DB
    if let Some(ref store) = session_store {
        match store.load_session(&session_id).await {
            Ok(messages) if !messages.is_empty() => {
                println!("\n📂 Cargando historial ({})...", session_id);
                for db_msg in &messages {
                    match crate::memory::serialization::db_to_rig(db_msg) {
                        Ok(rig_msg) => {
                            chat_history.push(rig_msg);
                            let icon = if db_msg.role == "user" {
                                "👤"
                            } else {
                                "🤖"
                            };
                            println!("{} > {}\n", icon, db_msg.content);
                        }
                        Err(e) => tracing::warn!("Error deserializando mensaje: {}", e),
                    }
                }
                // println!("--- Fin del historial ---\n");
            }
            _ => println!("📭 Sesión nueva.\n"),
        }
    }

    loop {
        print!("\n👤 > ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let query = input.trim();

        if query.eq_ignore_ascii_case("exit") || query.is_empty() {
            break Ok(());
        }

        // Inyección de contexto semántico
        let prompt_text = if let Some(injector) = &context_injector {
            let ctx = injector.build_context(query, 3).await;
            if ctx.trim().is_empty() {
                query.to_string()
            } else {
                format!("{}\n\n{}", query, ctx)
            }
        } else {
            query.to_string()
        };

        // AUTO-SAVE mensaje del usuario
        if let Some(ref store) = session_store {
            let _ = store
                .save_message(
                    &session_id,
                    &crate::memory::serialization::rig_to_db(
                        &session_id,
                        &RigMessage::user(query),
                    )?,
                )
                .await;
        }

        print!("🤖 > ");
        io::stdout().flush()?;

        // Usar .chat() con historial en vez de .prompt()
        match agent.chat(&prompt_text, chat_history.clone()).await {
            Ok(res) => {
                // AUTO-SAVE respuesta del agente
                if let Some(ref store) = session_store {
                    let _ = store
                        .save_message(
                            &session_id,
                            &crate::memory::serialization::rig_to_db(
                                &session_id,
                                &RigMessage::assistant(&res),
                            )?,
                        )
                        .await;
                }

                // Agregar respuesta al historial en memoria
                if let Ok(new_msg) = crate::memory::serialization::db_to_rig(
                    &crate::memory::serialization::rig_to_db(
                        &session_id,
                        &RigMessage::assistant(&res),
                    )?,
                ) {
                    chat_history.push(new_msg);
                }

                println!("{}", res);
            }
            Err(e) => eprintln!("\n❌ Error: {}", e),
        }
    }
}
