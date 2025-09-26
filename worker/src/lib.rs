use worker :: *;
use scraper::{Html, Selector};
use serde::{Serialize, Deserialize};
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

#[derive(Serialize, Clone)]
struct ApiResponse<T> {
    status: String,
    data: T,
}

#[derive(Serialize, Clone)]
struct QuoteError {
    code: String,
    error: String,
}

#[derive(Serialize, Clone)]
struct QuoteResponse {
    success: Vec<StockInfo>,
    failed: Vec<QuoteError>,
}

// --- Selector Management ---

#[derive(Debug, PartialEq, Clone, Copy)]
enum CodeType {
    Fx,
    Nikkei,
    Dji,
    Stock,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct SelectorConfig {
    name_selector: Option<String>,
    current_value_selector: Option<String>,
    previous_day_change_selector: Option<String>,
    change_rate_selector: Option<String>,
    update_time_selector: Option<String>,
    fx_item_selector: Option<String>,
    fx_term_selector: Option<String>,
    fx_price_selector: Option<String>,
}

fn get_code_type(code: &str) -> CodeType {
    if code.ends_with("=FX") {
        CodeType::Fx
    } else if code.starts_with("^N225") { // Nikkei
        CodeType::Nikkei
    } else if code.starts_with("^") { // Other indices like DJI
        CodeType::Dji
    } else {
        CodeType::Stock
    }
}

fn get_default_selectors(code_type: CodeType) -> SelectorConfig {
    match code_type {
        CodeType::Fx => SelectorConfig {
            name_selector: Some("title".to_string()), // FX uses title for name
            fx_item_selector: Some("div[class*=\"FxPriceBoard__item\"]".to_string()),
            fx_term_selector: Some("dt[class*=\"FxPriceBoard__term\"]".to_string()),
            fx_price_selector: Some("span[class*=\"FxPriceBoard__price\"]".to_string()),
            update_time_selector: Some("time".to_string()), // Generic time for FX
            ..Default::default()
        },
        CodeType::Dji | CodeType::Nikkei => SelectorConfig {
            name_selector: Some("h2[class*=\"_BasePriceBoard__name\"]".to_string()),
            current_value_selector: Some("span[class*=\"CommonPriceBoard__price\"] > span > span".to_string()),
            previous_day_change_selector: Some("span[class*=\"PriceChangeLabel__primary\"] > span".to_string()),
            change_rate_selector: Some("span[class*=\"PriceChangeLabel__secondary\"] > span[class*=\"StyledNumber__value\"]".to_string()),
            update_time_selector: Some("li[class*=\"_CommonPriceBoard__time\"] > time".to_string()), // Specific time for indices
            ..Default::default()
        },
        CodeType::Stock => SelectorConfig {
            name_selector: Some("h2[class*=\"PriceBoard__name\"]".to_string()),
            current_value_selector: Some("span[class*=\"PriceBoard__price\"] > span > span".to_string()),
            previous_day_change_selector: Some("span[class*=\"PriceChangeLabel__primary\"] > span".to_string()),
            change_rate_selector: Some("span[class*=\"PriceChangeLabel__secondary\"] > span[class*=\"StyledNumber__value\"]".to_string()),
            update_time_selector: Some("ul[class*=\"PriceBoard__times\"] > li > time".to_string()), // Specific time for stocks
            ..Default::default()
        },
    }
}

fn parse_price_values(
    price_s: String,
    change_s: String,
    percent_s: String,
) -> Result<(f64, f64, f64)> {
    let price = price_s.trim().replace(",", "").parse::<f64>();
    let change = change_s.trim().parse::<f64>();
    let percent = percent_s.trim().replace(&['(', ')', '%'][..], "").parse::<f64>();

    if let (Ok(price), Ok(change), Ok(percent)) = (price, change, percent) {
        Ok((price, change, percent))
    } else {
        Err("Failed to parse stock price values.".into())
    }
}

// 既存のget_stock_info関数 (株価用)
async fn get_regular_stock_info(code: String, selectors: &SelectorConfig) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}", code);
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

   

    let name_selector = Selector::parse(selectors.name_selector.as_ref().ok_or("Missing name selector for regular stock")?).unwrap();
    let name = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();


    let update_time_selector = Selector::parse(selectors.update_time_selector.as_ref().ok_or("Missing update time selector for regular stock")?).unwrap();
    let update_time = document.select(&update_time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();

    let price_selector = Selector::parse(selectors.current_value_selector.as_ref().ok_or("Missing price selector for regular stock")?).unwrap();
    let change_selector = Selector::parse(selectors.previous_day_change_selector.as_ref().ok_or("Missing change selector for regular stock")?).unwrap();
    let percent_selector = Selector::parse(selectors.change_rate_selector.as_ref().ok_or("Missing percent selector for regular stock")?).unwrap();

    if let (Some(price_s), Some(change_s), Some(percent_s)) = (
        document.select(&price_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&change_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&percent_selector).next().map(|e| e.text().collect::<String>()),
    ) {
        match parse_price_values(price_s, change_s, percent_s) {
            Ok((price, change, percent)) => Ok(StockInfo {
                code,
                name,
                price,
                change,
                change_percent: percent,
                update_time,
            }),
            Err(e) => Err(e),
        }
    } else {
        Err("Could not find all required stock price elements on the page.".into())
    }
}

async fn get_index_info(code: String, selectors: &SelectorConfig) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}", code);
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let name_selector = Selector::parse(selectors.name_selector.as_ref().ok_or("Missing name selector for index")?).unwrap();
    let name = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
    
    let update_time_selector = Selector::parse(selectors.update_time_selector.as_ref().ok_or("Missing update time selector for index")?).unwrap();
    let update_time = document.select(&update_time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
   

    let price_selector = Selector::parse(selectors.current_value_selector.as_ref().ok_or("Missing price selector for index")?).unwrap();
    let change_selector = Selector::parse(selectors.previous_day_change_selector.as_ref().ok_or("Missing change selector for index")?).unwrap();
    let percent_selector = Selector::parse(selectors.change_rate_selector.as_ref().ok_or("Missing percent selector for index")?).unwrap();

    if let (Some(price_s), Some(change_s), Some(percent_s)) = (
        document.select(&price_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&change_selector).next().map(|e| e.text().collect::<String>()),
        document.select(&percent_selector).next().map(|e| e.text().collect::<String>()),
    ) {
        match parse_price_values(price_s, change_s, percent_s) {
            Ok((price, change, percent)) => Ok(StockInfo {
                code,
                name,
                price,
                change,
                change_percent: percent,
                update_time,
            }),
            Err(e) => Err(e),
        }
    } else {
        Err("Could not find all required stock price elements on the page.".into())
    }
}

// 新しいget_fx_info関数 (FX用)
async fn get_fx_info(code: String, selectors: &SelectorConfig) -> Result<StockInfo> {
    let url = format!("https://finance.yahoo.co.jp/quote/{}/", code); // URLの末尾にスラッシュを追加
    let mut res = Fetch::Url(url.parse().unwrap()).send().await?;

    if res.status_code() != 200 {
        let status = res.status_code();
        let text = res.text().await.unwrap_or_else(|_| String::from("No body"));
        return Err(format!("Request failed for {} with status {}: {}", code, status, text).into());
    }

    let body = res.text().await?;
    let document = Html::parse_document(&body);

    let name_selector = Selector::parse(selectors.name_selector.as_ref().ok_or("Missing name selector for FX")?).unwrap();
    let name_full = document.select(&name_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();
    let name = name_full.split('【').next().unwrap_or("").trim().to_string();

    let time_selector = Selector::parse(selectors.update_time_selector.as_ref().ok_or("Missing update time selector for FX")?).unwrap();
    let update_time = document.select(&time_selector).next().map(|e| e.text().collect::<String>()).unwrap_or_default();

    let item_selector = Selector::parse(selectors.fx_item_selector.as_ref().ok_or("Missing FX item selector")?).unwrap();
    let price_value_selector = Selector::parse(selectors.fx_price_selector.as_ref().ok_or("Missing FX price selector")?).unwrap();
    let dt_selector = Selector::parse(selectors.fx_term_selector.as_ref().ok_or("Missing FX term selector")?).unwrap();

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
            for code in codes.clone() { // Clone codes to use in error reporting
                let decoded_code = decode(&code).unwrap_or_else(|_| Cow::Owned(code.clone())); // Decode the code
                let code_type = get_code_type(&decoded_code);
                let selectors = get_default_selectors(code_type);

                if decoded_code.ends_with("=FX") {
                    futures.push(get_fx_info(decoded_code.to_string(), &selectors).boxed_local());
                } else if decoded_code.starts_with("^") {
                    futures.push(get_index_info(decoded_code.to_string(), &selectors).boxed_local());
                } else {
                    futures.push(get_regular_stock_info(decoded_code.to_string(), &selectors).boxed_local());
                }
            }

            let results = join_all(futures).await;

            let mut successful_results: Vec<StockInfo> = Vec::new();
            let mut failed_results: Vec<QuoteError> = Vec::new();

            for (i, result) in results.into_iter().enumerate() { // Need to capture original code for error reporting
                let original_code = codes.get(i).map(|s| s.to_string()).unwrap_or_else(|| "unknown".to_string());
                match result {
                    Ok(stock_info) => successful_results.push(stock_info),
                    Err(e) => {
                        failed_results.push(QuoteError { code: original_code, error: e.to_string() });
                    }
                }
            }

            let status = if failed_results.is_empty() {
                "success".to_string()
            } else if successful_results.is_empty() {
                "failure".to_string()
            } else {
                "partial_success".to_string()
            };

            let api_response = ApiResponse {
                status,
                data: QuoteResponse {
                    success: successful_results,
                    failed: failed_results,
                },
            };

            Response::from_json(&api_response)
        })
        .run(req, env)
        .await
}