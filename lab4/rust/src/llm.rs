//! One helper for every Lab 4 experiment: a single model call, measured.
//!
//! Settings (environment variables, all optional):
//!   LAB_MODEL     the model (default gemini-3.5-flash-lite; fallback gemini-3.1-flash-lite, which has its own quota)
//!   LAB_TIMEOUT   seconds to wait for one answer (default 30); a timed-out request is tried 3 times in all
use crate::agent_core::model;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatResponse, JsonSpec};
use genai::Client;
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use std::fmt;
use std::time::{Duration, Instant};

const ATTEMPTS: u32 = 3;

pub struct Reply {
    pub text: String,
    pub in_tokens: i32,
    pub out_tokens: i32,
    pub seconds: f64,
}

/// Why a call failed. `Quota` will not fix itself today; `Timeout` might on the next call.
#[derive(Debug)]
pub enum LlmError {
    Quota(String),
    Timeout(String),
    Other(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::Quota(m) | LlmError::Timeout(m) | LlmError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<LlmError> for String {
    fn from(e: LlmError) -> String {
        e.to_string()
    }
}

fn timeout_seconds() -> f64 {
    std::env::var("LAB_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0)
}

/// One request with a timeout: fail fast on a daily quota, retry overloads, per-minute limits and timeouts.
async fn generate(client: &Client, req: &ChatRequest, opts: &ChatOptions) -> Result<ChatResponse, LlmError> {
    let limit = Duration::from_secs_f64(timeout_seconds());
    let mut wait = 2;
    for attempt in 1..=ATTEMPTS {
        let last = attempt == ATTEMPTS;
        match tokio::time::timeout(limit, client.exec_chat(&model(), req.clone(), Some(opts))).await {
            Ok(Ok(res)) => return Ok(res),
            Err(_) if last => {
                return Err(LlmError::Timeout(format!(
                    "No answer from {} within {} s, {ATTEMPTS} attempts (the service may be overloaded; \
                     set LAB_TIMEOUT to wait longer)",
                    model(),
                    timeout_seconds()
                )))
            }
            Err(_) => {} // timed out: try again
            Ok(Err(e)) => {
                let code = e.status().map(|s| s.as_u16()).unwrap_or(0);
                if code == 429 && e.to_string().contains("PerDay") {
                    return Err(LlmError::Quota(format!(
                        "The free tier's daily request limit for {} is used up (about 500 requests per day). \
                         Wait until it resets, use another API key, or switch model: set LAB_MODEL=gemini-3.1-flash-lite",
                        model()
                    )));
                }
                if last || !matches!(code, 408 | 429 | 500 | 502 | 503 | 504) {
                    return Err(LlmError::Other(e.to_string()));
                }
                // a per-minute limit clears quickly; an overload usually does too
                let secs = if code == 429 { 20 * attempt as u64 } else { wait };
                println!("  .. provider error {code}, retrying in {secs}s");
                tokio::time::sleep(Duration::from_secs(secs)).await;
                wait *= 2;
            }
        }
    }
    unreachable!()
}

/// One call. `system` is optional; `temperature: None` keeps the model default.
pub async fn ask(client: &Client, system: Option<&str>, prompt: &str, temperature: Option<f64>) -> Result<Reply, LlmError> {
    call(client, system, prompt, temperature, None).await
}

/// One call with structured output: the reply is parsed into `T` (a `JsonSchema` struct).
pub async fn ask_json<T: DeserializeOwned + JsonSchema>(
    client: &Client,
    system: Option<&str>,
    prompt: &str,
    temperature: Option<f64>,
) -> Result<(T, Reply), LlmError> {
    let schema = serde_json::to_value(schema_for!(T)).map_err(|e| LlmError::Other(e.to_string()))?;
    let reply = call(client, system, prompt, temperature, Some(JsonSpec::new("Output", schema))).await?;
    let parsed: T = serde_json::from_str(&reply.text).map_err(|e| LlmError::Other(format!("invalid JSON from the model: {e}")))?;
    Ok((parsed, reply))
}

async fn call(client: &Client, system: Option<&str>, prompt: &str, temperature: Option<f64>, spec: Option<JsonSpec>) -> Result<Reply, LlmError> {
    let mut messages = Vec::new();
    if let Some(s) = system {
        messages.push(ChatMessage::system(s));
    }
    messages.push(ChatMessage::user(prompt));
    let mut opts = ChatOptions::default();
    if let Some(t) = temperature {
        opts = opts.with_temperature(t);
    }
    if let Some(spec) = spec {
        opts = opts.with_response_format(spec);
    }
    let t0 = Instant::now();
    let res = generate(client, &ChatRequest::new(messages), &opts).await?;
    Ok(Reply {
        text: res.first_text().unwrap_or("").to_string(),
        in_tokens: res.usage.prompt_tokens.unwrap_or(0),
        out_tokens: res.usage.completion_tokens.unwrap_or(0),
        seconds: t0.elapsed().as_secs_f64(),
    })
}
