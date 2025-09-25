use worker :: *;
use scraper::{Html, Selector};
use serde::Serialize;
use futures::future::join_all;

#[derive(Serialize, Clone)]
struct StockInfo {
    code: String,
    price: f64,
    change: f64,
    change_percent: f64,
}

async fn get_stock_info(code: String) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}", code);
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let price_selector = Selector::parse("div[class*=\"PriceBoard__priceBlock\"] > span").unwrap();
    let change_selector = Selector::parse("span[class*=\"PriceChangeLabel__primary\"]").unwrap();
    let percent_selector = Selector::parse("span[class*=\"PriceChangeLabel__secondary\"]").unwrap();

    let price_str = document.select(&price_selector).next().map(|e| e.text().collect::<String>());
    let change_str = document.select(&change_selector).next().map(|e| e.text().collect::<String>());
    let percent_str = document.select(&percent_selector).next().map(|e| e.text().collect::<String>());

    if let (Some(price_s), Some(change_s), Some(percent_s)) = (price_str, change_str, percent_str) {
        let price = price_s.trim().replace(",", "").parse::<f64>();
        let change = change_s.trim().parse::<f64>();
        let percent = percent_s.trim().replace(&['(', ')', '%'][..], "").parse::<f64>();

        if let (Ok(price), Ok(change), Ok(percent)) = (price, change, percent) {
            Ok(StockInfo {
                code,
                price,
                change,
                change_percent: percent,
            })
        } else {
            Err("Failed to parse stock price values.".into())
        }
    } else {
        Err("Could not find all required stock price elements on the page.".into())
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .get_async("/", |_, _| async move {
            Response::ok("Hello from Workers!")
        })
        .get_async("/greet/:name", |_, ctx| async move {
            if let Some(name) = ctx.param("name") {
                Response::ok(format!("Hello, {}!", name))
            } else {
                Response::error("Bad Request", 400)
            }
        })
        .get_async("/echo/:word", |_, ctx| async move {
            if let Some(word) = ctx.param("word") {
                Response::ok(word.to_string())
            } else {
                Response::error("Bad Request", 400)
            }
        })
        .get_async("/stock/:codes", |_, ctx| async move {
            let codes_str = match ctx.param("codes") {
                Some(codes) => codes,
                None => return Response::error("Bad Request: Missing stock codes.", 400),
            };

            let codes: Vec<String> = codes_str.split(',').map(String::from).collect();
            
            let futures = codes.into_iter().map(get_stock_info);
            let results = join_all(futures).await;

            let successful_results: Vec<StockInfo> = results.into_iter().filter_map(Result::ok).collect();

            Response::from_json(&successful_results)
        })
        .run(req, env)
        .await
}
