use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize, Deserialize, Clone)]
struct Account {
    id: String,
    name: String,
    key: String,
    usage_str: String,
    status: String,
    primary_used: f64,
    secondary_used: f64,
    primary_reset_at: Option<i64>,   // unix timestamp for 5h reset
    secondary_reset_at: Option<i64>, // unix timestamp for weekly reset
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    polling_enabled: bool,
    polling_interval_secs: u64,
}

#[derive(Deserialize)]
struct AddAccountReq {
    name: String,
    key: String,
}

#[derive(Deserialize)]
struct RenameAccountReq {
    name: String,
}

#[derive(Serialize)]
struct StatusResp {
    tasks: usize,
    accounts: Vec<Account>,
}

#[derive(Deserialize, Debug)]
struct OpenAIBillingUsage {
    total_usage: f64,
}

#[derive(Deserialize, Debug)]
struct WhamRateLimitWindow {
    used_percent: f64,
    reset_at: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct WhamRateLimit {
    primary_window: Option<WhamRateLimitWindow>,
    secondary_window: Option<WhamRateLimitWindow>,
}

#[derive(Deserialize, Debug)]
struct WhamUsage {
    plan_type: Option<String>,
    rate_limit: Option<WhamRateLimit>,
}

async fn fetch_all_accounts(bg_accounts: Arc<Mutex<Vec<Account>>>) {
    let client = reqwest::Client::new();
    let accounts: Vec<Account> = {
        let guard = bg_accounts.lock().unwrap();
        guard.clone()
    };

    for acc in accounts {
        let mut new_usage = String::new();
        let mut new_status = String::new();
        let mut p_used = 0.0;
        let mut s_used = 0.0;
        let mut p_reset_at: Option<i64> = None;
        let mut s_reset_at: Option<i64> = None;

        let parsed_json: Option<serde_json::Value> = serde_json::from_str(&acc.key).ok();

        let mut access_token = acc.key.clone();
        let mut account_id = None;

        if let Some(json) = parsed_json {
            if let Some(token) = json.get("access_token").and_then(|v| v.as_str()) {
                access_token = token.to_string();
            } else if let Some(token) = json
                .pointer("/tokens/access_token")
                .and_then(|v| v.as_str())
            {
                access_token = token.to_string();
            }

            if let Some(acc_id) = json.get("account_id").and_then(|v| v.as_str()) {
                account_id = Some(acc_id.to_string());
            } else if let Some(acc_id) = json.pointer("/tokens/account_id").and_then(|v| v.as_str())
            {
                account_id = Some(acc_id.to_string());
            }
        }

        let mut is_wham = false;
        let wham_url = "https://chatgpt.com/backend-api/wham/usage";
        let mut req = client
            .get(wham_url)
            .bearer_auth(&access_token)
            .header("User-Agent", "codex-cli");

        if let Some(act_id) = account_id {
            req = req.header("ChatGPT-Account-Id", act_id);
        }

        let fallback_url = {
            let st = chrono::Local::now().format("%Y-%m-01").to_string();
            let ed = chrono::Local::now().format("%Y-%m-%d").to_string();
            format!(
                "https://api.openai.com/v1/dashboard/billing/usage?start_date={}&end_date={}",
                st, ed
            )
        };

        if let Ok(res) = req.send().await {
            if res.status().is_success() {
                is_wham = true;
                if let Ok(json) = res.json::<WhamUsage>().await {
                    new_status = "Active".to_string();
                    new_usage = json
                        .plan_type
                        .unwrap_or_else(|| "ChatGPT".into())
                        .to_uppercase();

                    if let Some(limit) = json.rate_limit {
                        p_used = limit.primary_window.as_ref().map(|w| w.used_percent).unwrap_or(0.0);
                        s_used = limit.secondary_window.as_ref().map(|w| w.used_percent).unwrap_or(0.0);
                        p_reset_at = limit.primary_window.as_ref().and_then(|w| w.reset_at);
                        s_reset_at = limit.secondary_window.as_ref().and_then(|w| w.reset_at);
                    }
                }
            }
        }

        if !is_wham {
            if let Ok(res) = client
                .get(&fallback_url)
                .bearer_auth(&access_token)
                .send()
                .await
            {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<OpenAIBillingUsage>().await {
                        new_usage = format!("${:.2}", json.total_usage / 100.0);
                        new_status = "Active".to_string();
                    }
                } else if res.status() == reqwest::StatusCode::UNAUTHORIZED {
                    new_status = "Invalid Key".to_string();
                } else {
                    new_status = "API Sync Error".to_string();
                }
            } else {
                new_status = "Network Error".to_string();
            }
        }

        {
            let mut guard = bg_accounts.lock().unwrap();
            if let Some(a) = guard.iter_mut().find(|a| a.id == acc.id) {
                if !new_usage.is_empty() {
                    a.usage_str = new_usage;
                }
                a.status = new_status;
                a.primary_used = p_used;
                a.secondary_used = s_used;
                a.primary_reset_at = p_reset_at;
                a.secondary_reset_at = s_reset_at;
            }
        }
    }

    if let Ok(guard) = bg_accounts.lock() {
        if let Ok(json_str) = serde_json::to_string_pretty(&*guard) {
            let _ = fs::write("accounts.json", json_str);
        }
    }
}

#[tokio::main]
async fn main() {
    let default_accounts: Vec<Account> = vec![];
    let accounts_data = match fs::read_to_string("accounts.json") {
        Ok(data) => serde_json::from_str(&data).unwrap_or(default_accounts),
        Err(_) => default_accounts,
    };
    let state_accounts = Arc::new(Mutex::new(accounts_data));

    let default_settings = Settings {
        polling_enabled: true,
        polling_interval_secs: 15,
    };
    let settings_data = match fs::read_to_string("settings.json") {
        Ok(data) => serde_json::from_str(&data).unwrap_or(default_settings.clone()),
        Err(_) => default_settings,
    };
    let state_settings = Arc::new(Mutex::new(settings_data));

    // Used for explicit refresh requests
    let explicit_refresh_notify = Arc::new(Notify::new());

    let bg_accounts = Arc::clone(&state_accounts);
    let bg_settings = Arc::clone(&state_settings);
    let bg_notify = Arc::clone(&explicit_refresh_notify);

    tokio::spawn(async move {
        // Initial load immediately
        fetch_all_accounts(Arc::clone(&bg_accounts)).await;

        loop {
            let (enabled, interval) = {
                let guard = bg_settings.lock().unwrap();
                (guard.polling_enabled, guard.polling_interval_secs)
            };

            if enabled {
                tokio::select! {
                    _ = bg_notify.notified() => {
                        // User clicked refresh
                        fetch_all_accounts(Arc::clone(&bg_accounts)).await;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(interval.max(2))) => {
                        // Interval elapsed
                        fetch_all_accounts(Arc::clone(&bg_accounts)).await;
                    }
                }
            } else {
                tokio::select! {
                    _ = bg_notify.notified() => {
                        fetch_all_accounts(Arc::clone(&bg_accounts)).await;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                        // Wake up periodically just to check if settings changed
                    }
                }
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_status = {
        let acc_cl = Arc::clone(&state_accounts);
        move || async move {
            let masked_accs = acc_cl.lock().unwrap().clone();
            Json(StatusResp {
                tasks: masked_accs.len(),
                accounts: masked_accs,
            })
        }
    };

    let api_add_account = {
        let acc_cl = Arc::clone(&state_accounts);
        let notify_cl = Arc::clone(&explicit_refresh_notify);
        move |Json(payload): Json<AddAccountReq>| async move {
            let mut guard = acc_cl.lock().unwrap();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string();

            guard.insert(
                0,
                Account {
                    id: timestamp,
                    name: payload.name,
                    key: payload.key,
                    usage_str: "$0.00".to_string(),
                    status: "Pending...".to_string(),
                    primary_used: 0.0,
                    secondary_used: 0.0,
                    primary_reset_at: None,
                    secondary_reset_at: None,
                },
            );
            let _ = fs::write(
                "accounts.json",
                serde_json::to_string_pretty(&*guard).unwrap(),
            );
            drop(guard);

            // Notify background task to fetch immediately for the new account
            notify_cl.notify_one();
            Json(serde_json::json!({"status": "ok"}))
        }
    };

    let api_del_account = {
        let acc_cl = Arc::clone(&state_accounts);
        move |Path(id): Path<String>| async move {
            let mut guard = acc_cl.lock().unwrap();
            guard.retain(|a| a.id != id);
            let _ = fs::write(
                "accounts.json",
                serde_json::to_string_pretty(&*guard).unwrap(),
            );
            Json(serde_json::json!({"status": "ok"}))
        }
    };

    let api_rename_account = {
        let acc_cl = Arc::clone(&state_accounts);
        move |Path(id): Path<String>, Json(payload): Json<RenameAccountReq>| async move {
            let mut guard = acc_cl.lock().unwrap();
            if let Some(a) = guard.iter_mut().find(|a| a.id == id) {
                a.name = payload.name;
            }
            let _ = fs::write(
                "accounts.json",
                serde_json::to_string_pretty(&*guard).unwrap(),
            );
            Json(serde_json::json!({"status": "ok"}))
        }
    };

    let api_explicit_refresh = {
        let notify_cl = Arc::clone(&explicit_refresh_notify);
        move || async move {
            notify_cl.notify_one();
            Json(serde_json::json!({"status": "ok"}))
        }
    };

    let api_get_settings = {
        let set_cl = Arc::clone(&state_settings);
        move || async move {
            let guard = set_cl.lock().unwrap();
            Json(guard.clone())
        }
    };

    let api_set_settings = {
        let set_cl = Arc::clone(&state_settings);
        let notify_cl = Arc::clone(&explicit_refresh_notify);
        move |Json(payload): Json<Settings>| async move {
            let mut guard = set_cl.lock().unwrap();
            *guard = payload;
            let _ = fs::write(
                "settings.json",
                serde_json::to_string_pretty(&*guard).unwrap(),
            );

            // Wake up background thread so it can apply new polling interval immediately
            notify_cl.notify_one();
            Json(serde_json::json!({"status": "ok"}))
        }
    };

    let app = Router::new()
        .route("/status", get(api_status))
        .route("/accounts", post(api_add_account))
        .route("/accounts/:id", delete(api_del_account))
        .route("/accounts/:id/rename", post(api_rename_account))
        .route("/refresh", post(api_explicit_refresh))
        .route("/settings", get(api_get_settings).post(api_set_settings))
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], 48123));
    println!("Backend listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
