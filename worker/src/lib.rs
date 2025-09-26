use worker :: *;
use scraper::{Html, Selector};
use serde::Serialize;
use futures::future::{join_all, FutureExt}; // FutureExt を追加
use std::pin::Pin;
use futures::Future;
use urlencoding::decode;
use std::borrow::Cow;

#[derive(Serialize, Clone)]
struct StockInfo {
    code: String,
    name: String,
    price: f64,
    change: f64,
    change_percent: f64,
    update_time: String,
}

// 既存のget_stock_info関数 (株価用)
async fn get_regular_stock_info(code: String) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}", code);
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

   

    let name_selector = Selector::parse("h2[class*=\"PriceBoard__name\"]").unwrap();
    let name = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();


    let update_time_selector = Selector::parse("ul[class*=\"PriceBoard__times\"] > li > time").unwrap();
    let update_time = document.select(&update_time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();

    let price_selector = Selector::parse("span[class*=\"PriceBoard__price\"] > span > span").unwrap();
    let change_selector = Selector::parse("span[class*=\"PriceChangeLabel__primary\"] > span").unwrap();
    let percent_selector = Selector::parse("span[class*=\"PriceChangeLabel__secondary\"] > span[class*=\"StyledNumber__value\"]").unwrap();

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
                name,
                price,
                change,
                change_percent: percent,
                update_time,
            })
        } else {
            Err("Failed to parse stock price values.".into())
        }
    } else {
        Err("Could not find all required stock price elements on the page.".into())
    }
}

async fn get_index_info(code: String) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}", code);
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let name_selector = Selector::parse("h2[class*=\"_BasePriceBoard__name\"]").unwrap();
    let name = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
    
    let time_selector = Selector::parse("time").unwrap();
    let update_time = document.select(&time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
   

    let price_selector = Selector::parse("span[class*=\"CommonPriceBoard__price\"] > span > span").unwrap();
    let change_selector = Selector::parse("span[class*=\"PriceChangeLabel__primary\"] > span").unwrap();
    let percent_selector = Selector::parse("span[class*=\"PriceChangeLabel__secondary\"] > span[class*=\"StyledNumber__value\"]").unwrap();

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
                name,
                price,
                change,
                change_percent: percent,
                update_time,
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
    let url = format!("https://finance.yahoo.co.jp/quote/{}/", code); // URLの末尾にスラッシュを追加
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let name_selector = Selector::parse("title").unwrap();
    let name_full = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
    let name = name_full.split('【').next().unwrap_or("").trim().to_string();

    let time_selector = Selector::parse("time").unwrap();
    let update_time = document.select(&time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();

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
            name,
            price: p,
            change: c,
            change_percent: 0.0, // FXページには直接的な変化率がないため、0.0とする
            update_time,
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
            
            let mut futures: Vec<Pin<Box<dyn Future<Output = Result<StockInfo>>>>> = Vec::new();
            for code in codes {
                let decoded_code = decode(&code).unwrap_or_else(|_| Cow::Owned(code.clone())); // Decode the code
                if decoded_code.ends_with("=FX") {
                    futures.push(get_fx_info(decoded_code.to_string()).boxed_local());
                } else if decoded_code.starts_with("^") {
                    futures.push(get_index_info(decoded_code.to_string()).boxed_local());
                } else {
                    futures.push(get_regular_stock_info(decoded_code.to_string()).boxed_local());
                }
            }

            let results = join_all(futures).await;

            let successful_results: Vec<StockInfo> = results.into_iter().filter_map(Result::ok).collect();

            Response::from_json(&successful_results)
        })
        .run(req, env)
        .await
}