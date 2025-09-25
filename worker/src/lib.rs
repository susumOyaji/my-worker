use worker :: *;
use scraper::{Html, Selector};
use serde::Serialize;
use futures::future::{join_all, FutureExt}; // FutureExt を追加
// use std::pin::Pin; // 不要になる
// use futures::Future; // 不要になる

#[derive(Serialize, Clone)]
struct StockInfo {
    code: String,
    price: f64,
    change: f64,
    change_percent: f64,
}

// 既存のget_stock_info関数 (株価用)
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

    if let (Some(price_s), Some(change_s), Some(percent_s)) = (
        document.select(&price_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&change_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&percent_selector).next().map(|e| e.text().collect::<String>()),
    ) {
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

// 新しいget_fx_info関数 (FX用)
async fn get_fx_info(code: String) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}/
", code); // URLの末尾にスラッシュを追加
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let item_selector = Selector::parse("div[class*=\"FxPriceBoard__item\"]").unwrap();
    let price_value_selector = Selector::parse("span[class*=\"FxPriceBoard__price\"]").unwrap();
    let dt_selector = Selector::parse("dt[class*=\"FxPriceBoard__term\"]").unwrap();

    let mut price: Option<f64> = None;
    let mut change: Option<f64> = None;

    for item_div in document.select(&item_selector) {
        if let Some(dt_element) = item_div.select(&dt_selector).next() {
            let term_text = dt_element.text().collect::<String>();
            if let Some(price_span) = item_div.select(&price_value_selector).next() {
                let price_str = price_span.text().collect::<String>();
                let parsed_price = price_str.trim().replace(",", "").replace("<!-- -->", "").parse::<f64>();

                if parsed_price.is_ok() {
                    if term_text.contains("Bid（売値）") {
                        price = Some(parsed_price.unwrap());
                    } else if term_text.contains("Change（始値比）") {
                        change = Some(parsed_price.unwrap());
                    }
                }
            }
        }
    }

    if let (Some(p), Some(c)) = (price, change) {
        Ok(StockInfo {
            code,
            price: p,
            change: c,
            change_percent: 0.0, // FXページには直接的な変化率がないため、0.0とする
        })
    } else {
        Err(format!("Could not find all required FX price elements on the page for code: {}", code).into())
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
            
            let mut futures = Vec::new(); // 型指定は不要になる
            for code in codes {
                if code.ends_with("=FX") {
                    futures.push(get_fx_info(code).boxed()); // .boxed() を使用
                } else {
                    futures.push(get_stock_info(code).boxed()); // .boxed() を使用
                }
            }

            let results = join_all(futures).await;

            let successful_results: Vec<StockInfo> = results.into_iter().filter_map(Result::ok).collect();

            Response::from_json(&successful_results)
        })
        .run(req, env)
        .await
}