//! NVIDIA API client para Rig - Implementación externa con tool calling
//! Endpoint: https://integrate.api.nvidia.com/v1/chat/completions

use anyhow::Result;
use rig::completion::{
    CompletionError, CompletionRequest, CompletionResponse, Message as RigMessage,
};
use rig::message::{AssistantContent, Text, ToolCall, UserContent};
use rig::streaming::StreamingCompletionResponse;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::future::Future;

// ==================== Client mínimo ====================
#[derive(Clone)]
pub struct Client {
    api_key: String,
    base_url: String,
    http_client: reqwest::Client,
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://integrate.api.nvidia.com/v1".into(),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        self.http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
    }
}

// ==================== CompletionModel ====================
#[derive(Clone)]
pub struct CompletionModel {
    client: Client,
    model: String,
}

impl CompletionModel {
    pub fn new(client: Client, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }
}

// Response types (OpenAI-compatible)
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct NvidiaResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Choice {
    pub index: usize,
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct AssistantMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct OpenAIToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

impl rig::completion::GetTokenUsage for NvidiaResponse {
    fn token_usage(&self) -> Option<rig::completion::Usage> {
        self.usage.as_ref().map(|u| rig::completion::Usage {
            input_tokens: u.prompt_tokens as u64,
            output_tokens: u.completion_tokens as u64,
            total_tokens: u.total_tokens as u64,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
        })
    }
}

// ==================== Implementación del trait CompletionModel ====================
impl rig::completion::CompletionModel for CompletionModel {
    type Response = NvidiaResponse;
    type StreamingResponse = NvidiaResponse;
    type Client = Client;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        Self::new(client.clone(), model)
    }

    fn completion(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse<Self::Response>, CompletionError>> + Send
    {
        async move {
            // 1. Construir body para NVIDIA (con tool calling)
            let body = build_nvidia_body(&self.model, &request).map_err(|e| {
                CompletionError::RequestError(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()).into(),
                )
            })?;

            // tracing::debug!("📤 NVIDIA REQUEST | model: {} | tools_count: {}", self.model, request.tools.len());

            // tracing::info!("🔍 NVIDIA FULL REQUEST PAYLOAD:\n{}", serde_json::to_string_pretty(&body).unwrap_or_else(|_| "Failed to serialize".into()));

            // 2. HTTP POST
            let resp = self
                .client
                .post("chat/completions")
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    CompletionError::RequestError(
                        std::io::Error::new(std::io::ErrorKind::Other, e.to_string()).into(),
                    )
                })?;

            // 3. Check status
            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let text = resp.text().await.unwrap_or_else(|_| "Unknown error".into());
                return Err(CompletionError::ProviderError(format!(
                    "HTTP {}: {}",
                    status, text
                )));
            }

            // 4. Parsear respuesta
            let nvidia_resp: NvidiaResponse = resp.json().await.map_err(|e| {
                CompletionError::RequestError(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()).into(),
                )
            })?;

            let message_id = nvidia_resp.id.clone();

            // 5. Convertir a CompletionResponse de Rig (con tool calling)
            let first_choice = nvidia_resp
                .choices
                .first()
                .ok_or_else(|| CompletionError::ProviderError("No choices in response".into()))?;

            // ¿Hay tool_calls en la respuesta?
            let choice_content = if let Some(tool_calls) = &first_choice.message.tool_calls {
                // Convertir OpenAI tool_calls a Rig ToolCall
                let tool_calls_rig: Vec<AssistantContent> = tool_calls
                    .iter()
                    .filter_map(|tc| {
                        // Parsear argumentos JSON
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).ok()?;
                        Some(AssistantContent::ToolCall(ToolCall {
                            id: tc.id.clone(),
                            call_id: None,
                            function: rig::message::ToolFunction::new(
                                tc.function.name.clone(),
                                args,
                            ),
                            signature: None,
                            additional_params: None,
                        }))
                    })
                    .collect();

                if tool_calls_rig.is_empty() {
                    // Fallback a texto si no pudimos parsear tool calls
                    let content = first_choice.message.content.clone().unwrap_or_default();
                    vec![AssistantContent::Text(Text { text: content })]
                } else {
                    tool_calls_rig
                }
            } else {
                // Respuesta normal de texto
                let content = first_choice.message.content.clone().unwrap_or_default();
                vec![AssistantContent::Text(Text { text: content })]
            };

            tracing::debug!(
                "📥 NVIDIA RESPONSE | id: {} | finish: {:?} | has_tool_calls: {}",
                nvidia_resp.id,
                first_choice.finish_reason,
                first_choice.message.tool_calls.is_some()
            );

            Ok(CompletionResponse {
                choice: rig::OneOrMany::many(choice_content)
                    .map_err(|_| CompletionError::ProviderError("Empty choice content".into()))?,
                usage: nvidia_resp.usage.as_ref().map_or(
                    rig::completion::Usage {
                        input_tokens: 0,
                        output_tokens: 0,
                        total_tokens: 0,
                        cached_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                    },
                    |u| rig::completion::Usage {
                        input_tokens: u.prompt_tokens as u64,
                        output_tokens: u.completion_tokens as u64,
                        total_tokens: u.total_tokens as u64,
                        cached_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                    },
                ),
                raw_response: nvidia_resp,
                message_id: Some(message_id),
            })
        }
    }

    fn stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<
        Output = Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>,
    > + Send {
        async move {
            Err(CompletionError::ProviderError(
                "Streaming not implemented for NVIDIA provider".into(),
            ))
        }
    }
}

// ==================== Helper: construir body con tool calling ====================
fn build_nvidia_body(
    model: &str,
    request: &CompletionRequest,
) -> Result<serde_json::Value, anyhow::Error> {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    // 1. Agregar preamble como system message
    if let Some(preamble) = &request.preamble {
        messages.push(json!({ "role": "system", "content": preamble }));
    }

    // 2. Agregar chat_history
    for msg in request.chat_history.iter() {
        match msg {
            RigMessage::System { content } => {
                messages.push(json!({ "role": "system", "content": content }));
            }

            RigMessage::User { content } => {
                for c in content.iter() {
                    match c {
                        UserContent::Text(Text { text }) => {
                            messages.push(json!({ "role": "user", "content": text }));
                        }
                        UserContent::ToolResult(tr) => {
                            let tool_call_id = tr.call_id.clone().unwrap_or_else(|| tr.id.clone());
                            let result_text = tr
                                .content
                                .iter()
                                .filter_map(|tc| match tc {
                                    rig::message::ToolResultContent::Text(Text { text }) => {
                                        Some(text.clone())
                                    }
                                    _ => None, // ← Wildcard para ToolResultContent (Image, etc.)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_call_id,
                                "content": result_text
                            }));
                        }
                        _ => {}
                    }
                }
            }

            RigMessage::Assistant { content, .. } => {
                // Extraer texto, pero loguear si hay tool_calls o reasoning
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        AssistantContent::Text(Text { text }) => Some(text.clone()),
                        AssistantContent::ToolCall(tc) => {
                            // 🔍 LOG: ver tool_call que Rig quiere enviar
                            tracing::debug!(
                                "🔧 AssistantContent::ToolCall detected: name={}, id={}",
                                tc.function.name,
                                tc.id
                            );
                            None // No incluir tool_calls en "content", van en campo separado
                        }
                        other => {
                            tracing::debug!(
                                "⚠️ AssistantContent no-text: {:?}",
                                std::mem::discriminant(other)
                            );
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(json!({ "role": "assistant", "content": text }));
            }
        }
    }

    // ver cuántos mensajes y si hay vacíos
    for (i, msg) in messages.iter().enumerate() {
        if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                // Solo warnear si es user/system con contenido vacío (tool results vacíos sí son problema)
                if content.is_empty() && role != "assistant" {
                    tracing::warn!("⚠️ Mensaje {} con role '{}' tiene contenido vacío", i, role);
                }
            }
        }
    }

    // 3. Construir body principal
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": request.temperature.unwrap_or(0.7),
        "max_tokens": request.max_tokens.unwrap_or(1024),
        "stream": false
    });

    // 4. Tool calling (formato OpenAI-compatible)
    if !request.tools.is_empty() {
        let tools_openai: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters
                    }
                })
            })
            .collect();
        body["tools"] = serde_json::to_value(tools_openai)?;
    }

    if let Some(tool_choice) = &request.tool_choice {
        body["tool_choice"] = serde_json::to_value(tool_choice)?;
    }

    // 5. Merge additional_params
    if let Some(extra) = &request.additional_params {
        if let (Some(obj), serde_json::Value::Object(extra_obj)) = (body.as_object_mut(), extra) {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    Ok(body)
}

// ==================== Debug seguro ====================
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NvidiaClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"<REDACTED>")
            .finish()
    }
}
