# docs/AGENT.md

## Árbol del proyecto

```
src/
├── main.rs                    → Entry point: carga config, init DB, embeddings, build_agent, run_tui
├── config.rs                  → Config struct (desde .env): API keys, modelos, max_context_messages (default 20), timeouts
├── agent/
│   ├── mod.rs                 → type alias AppAgent = rig::agent::Agent<OpenAI CompletionModel<reqwest::Client>>
│   ├── builder.rs             → build_agent(): crea cliente OpenAI-compatible, preamble, tools, dynamic context, temperature 0.2
│   ├── hooks.rs               → AgentHooks: eventos (PromptSent, StreamingChunk, ToolCalled, etc.) + cancel_flag
│   └── service.rs             → AgentService: wrapper simple agent.prompt().send()
├── context/
│   ├── mod.rs
│   ├── chunker.rs             → chunk_text(): divide texto en chunks de max_len
│   ├── file_cache.rs          → Cache persistente SQLite para archivos @-referenciados (con detección de cambios en disco)
│   ├── file_loader.rs         → read_file_text(), discover_files(), extract_file_references() (@-parsing), get_project_root()
│   ├── injector.rs            → (comentado) ContextInjector: búsqueda semántica de facts vía embeddings
│   └── vector_store.rs        → AppVectorStore: wrapper sobre InMemoryVectorStore de Rig
├── io/
│   ├── mod.rs
│   ├── cli_loop.rs            → run_cli(): loop stdin/stdout con historial, context injection, auto-save a DB
│   └── tui/
│       ├── mod.rs             → re-exporta run_tui
│       ├── app.rs             → TuiApp: estado de la UI, input, scroll, envío al agente con truncado de historial
│       ├── commands.rs        → Command enum + parse(): /new, /copy, /rename, /help, /exit, etc.
│       ├── events.rs          → run_tui(): loop principal con crossterm, mouse capture, paste, teclas
│       ├── render.rs          → render(): layout vertical (chat + input + status), scrollbar, colores por rol
│       └── session_select.rs  → select_session(): menú TUI para elegir sesión existente o crear nueva
├── memory/
│   ├── mod.rs
│   ├── session_store.rs       → SessionStore trait: list_sessions, load_session, save_message, delete, update_name
│   ├── sqlite.rs              → MemoryDB: implementación SQLite de SessionStore + save_fact, search_facts, file_cache, embeddings
│   ├── serialization.rs       → rig_to_db() / db_to_rig(): conversión entre rig::completion::Message y DbMessage (JSON)
│   └── in_memory_store.rs     → InMemorySessionStore: implementación en memoria del trait SessionStore (para tests/mock)
├── providers/
│   ├── mod.rs
│   ├── nvidia.rs              → Cliente NVIDIA API (OpenAI-compatible) con tool calling, sin streaming
│   └── local.rs               → Wrapper para llama.cpp local (OpenAI-compatible)
├── state/
│   ├── mod.rs
│   └── conversation.rs        → ConversationState, MessageRole (User/Assistant/System), UiMessage
├── tools/
│   ├── mod.rs                 → Re-exporta ApplyDiffTool, ReadDirTool, ReadFileTool, WriteFileTool
│   ├── apply_diff.rs          → Tool: SEARCH/REPLACE diff con backup .bak, múltiples bloques, preview
│   ├── read_dir.rs            → Tool: lista archivos/dirs respetando .gitignore, exclude dirs ruidosos, max_depth, status TUI
│   ├── read_file.rs           → Tool: lee archivos con cache SQLite persistente
│   ├── write_file.rs          → Tool: crea archivos o sobrescribe con confirm_overwrite=true y backup .bak
│   ├── memory/
│   │   ├── mod.rs
│   │   ├── recall.rs          → Tool: recall_memory(query) → carga facts de DB por topic (LIKE 'topic_%')
│   │   └── save_fact.rs       → Tool: save_fact(key, value, context) → guarda en tabla facts
│   └── web/
│       ├── mod.rs
│       └── katana.rs          → Tool: katana_crawl(url) → stub, llama a katana externo
└── utils/
    ├── mod.rs
    ├── clipboard.rs           → copy_to_clipboard(), extract_code_blocks(), get_code_block()
    ├── tracing.rs             → init_tracing(), debug_log() a /tmp/rustclaw-debug.log
    └── visual_mode.rs         → export_conversation_to_markdown(), run_visual_mode(): suspende TUI, abre editor externo
```

## Flujo de app.rs (TUI)

1. **`TuiApp::new()`** → recibe agente, mensajes iniciales (UI + Rig), session_id, memory_db. Crea canal mpsc para eventos async.
2. **`handle_submit()`** → toma el texto del input, lo parsea como `Command`. Si es `Message`, llama a `send_to_agent()`.
3. **`send_to_agent(raw_input)`**:
   - Limpia el input (saca bloques `--- USER CONTEXT ---` si existen).
   - Agrega mensaje `User` a `conversation.messages` (UI).
   - Extrae referencias `@archivo` con `extract_file_references()`.
   - Para cada referencia: intenta cache (SQLite) → si miss, lee disco y guarda en cache. Inyecta `[FILE: path]\ncontent\n[/FILE]` al prompt.
   - Agrega mensaje `System` "Thinking..." a la UI.
   - **Trunca el historial**: toma `config.max_context_messages` (default 20) últimos mensajes de `self.chat_history` y se los pasa al agente. El historial completo se conserva en UI.
   - Spawnea una tarea async que bloquea el agente, emite `ToolStatus` (`agent`) y llama a `agent.chat(enriched_prompt, truncated_history)` con timeout dinámico (`timeout_base_secs * max_turns`).
   - Al recibir respuesta: guarda el mensaje del usuario en DB y envía `UiEvent::AgentResponse` + `UiEvent::NewRigMessage` por el canal.
4. **`poll_agent_events()`** → drena el canal `ui_rx`:
   - `AgentResponse`: reemplaza "Thinking..." por la respuesta, agrega a UI, guarda en DB.
   - `AgentError`: muestra error en UI.
   - `NewRigMessage`: agrega al `chat_history` en memoria.
   - `ToolStatus`: reemplaza "Thinking..." si corresponde y agrega un mensaje de sistema `[tool] status`.
5. **`execute(cmd)`** → maneja comandos: `/copy`, `/rename`, `/help`, `exit`.

## Config relevante (config.rs)

| Variable | Default | Descripción |
|---|---|---|
| `max_context_messages` | 20 | Últimos N mensajes enviados al agente (excluye system prompt) |
| `max_turns` | 5 | Iteraciones internas máximas por mensaje del usuario |
| `timeout_base_secs` | 40 | Timeout base por iteración (timeout total = base * max_turns) |
| `max_context` | 3 | Documentos del dynamic context (RAG) |

## Tools del agente (builder.rs)

- `katana_crawl` → crawlea URLs (stub, llama a katana externo)
- `save_fact` → guarda reglas/preferencias en DB
- `recall_memory` → carga facts por topic (ej: "coding", "linux")
- `read_file` → lee archivos con cache SQLite
- `read_dir` → lista estructura de directorios y reporta estado al TUI
- `apply_diff` → aplica SEARCH/REPLACE diffs con backup .bak
- `write_file` → crea archivos nuevos o sobrescribe con confirmación y backup .bak

El preamble del agente incluye facts globales desde DB (`general_*`) y reglas breves: recall solo para temas específicos, responder en el idioma del usuario y evitar meta-comentarios/proceso. El LLM se instancia con `rig::providers::openai::CompletionsClient` apuntando a `NVIDIA_BASE_URL`; el cliente NVIDIA propio queda comentado.
