//! Visual Mode: suspender TUI y abrir editor externo para scroll+selección

use crate::state::conversation::{MessageRole, UiMessage};
use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use std::fs;
use std::io::Write;
use std::process::Command;

/// Formatea el historial de conversación como Markdown legible
pub fn export_conversation_to_markdown(messages: &[UiMessage]) -> String {
    let mut md = String::from("# Conversation History\n\n");

    for msg in messages {
        let role_icon = match msg.role {
            MessageRole::User => "👤",
            MessageRole::Assistant => "🤖",
            MessageRole::System => "⚙️",
        };
        let timestamp = msg.timestamp.format("%Y-%m-%d %H:%M:%S");

        md.push_str(&format!("### {} [{}]\n\n", role_icon, timestamp));
        md.push_str(&msg.content);
        md.push_str("\n\n---\n\n");
    }

    md
}

/// Suspende la TUI, abre un editor externo con el contenido, y reanuda
/// ← Solo maneja terminal state (alternate screen + raw mode), NO mouse capture
pub fn run_visual_mode<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    content: &str,
    editor: Option<&str>,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    // 1. Crear archivo temporal único para el contenido
    let temp_path = std::env::temp_dir().join(format!("rustclaw-{}.md", uuid::Uuid::new_v4()));
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(content.as_bytes())?;
    drop(file);

    // 2. Suspender TUI: solo terminal state
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    // 3. Resolver editor (micro → $EDITOR → vim/nano)
    let editor_cmd: String = if let Some(ed) = editor {
        ed.to_string()
    } else {
        let micro_available = std::process::Command::new("micro")
            .arg("--version")
            .output()
            .is_ok();

        if micro_available {
            "micro".to_string()
        } else if let Ok(ed) = std::env::var("EDITOR") {
            ed
        } else {
            ["vim", "nano"]
                .iter()
                .find(|&cmd| {
                    std::process::Command::new(cmd)
                        .arg("--version")
                        .output()
                        .is_ok()
                })
                .map(|s| s.to_string())
                .unwrap_or_else(|| "micro".to_string())
        }
    };

    // Verificar que el editor existe
    if std::process::Command::new(&editor_cmd)
        .arg("--version")
        .output()
        .is_err()
    {
        // ← Reanudar SOLO terminal state (sin mouse)
        enable_raw_mode()?;
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        let _ = terminal.clear();
        return Err(anyhow::anyhow!(
            "Editor '{}' not found in $PATH. Install micro, vim, or nano, or set $EDITOR",
            editor_cmd
        ));
    }

    // 4. Preparar args y config temporal
    let (mut args, temp_config_dir): (Vec<String>, Option<std::path::PathBuf>) =
        match editor_cmd.as_str() {
            "micro" => {
                let config_dir =
                    std::env::temp_dir().join(format!("micro-config-{}", uuid::Uuid::new_v4()));
                let bindings_path = config_dir.join("bindings.json");

                let _ = fs::create_dir_all(&config_dir);
                let _ = fs::write(
                    &bindings_path,
                    r#"{"buffer": {"q": "Quit", "Esc": "Quit"}}"#,
                );

                let args = vec!["-readonly=true".to_string(), "-softwrap=true".to_string()];
                (args, Some(config_dir))
            }
            "vim" | "vi" => (
                vec!["-R".to_string(), "-c".to_string(), "set wrap".to_string()],
                None,
            ),
            "nano" => (vec!["-R".to_string()], None),
            _ => (vec![], None),
        };

    args.push(temp_path.to_string_lossy().into_owned());

    let mut cmd = Command::new(&editor_cmd);
    if editor_cmd == "micro"
        && let Some(ref config_dir) = temp_config_dir
    {
        cmd.env("MICRO_CONFIG_HOME", config_dir);
    }

    // Lanzar editor
    let status = cmd.args(&args).status();

    // 5. Reanudar TUI: SOLO terminal state (sin mouse)
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let _ = terminal.clear();

    // 6. Cleanup
    let _ = fs::remove_file(&temp_path);
    if let Some(config_dir) = temp_config_dir {
        let _ = fs::remove_dir_all(&config_dir);
    }

    status?;
    Ok(())
}
