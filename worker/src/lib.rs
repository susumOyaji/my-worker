use worker::*;
use serde_json::json;

#[event(fetch)]
pub async fn main(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let secret_value = env
        .var("MY_VARIABLE")
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "undefined".to_string());

    let json_value = json!({
        "message": "Hello from Rust Worker!",
        "env_name": secret_value
    });

    Response::from_json(&json_value)
}