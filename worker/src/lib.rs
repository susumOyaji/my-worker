use worker::*;

#[event(fetch)]
pub async fn main(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;

    let name = url.query_pairs()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| "World".to_string());

    Response::ok(format!("Echoed: {}", name))
}