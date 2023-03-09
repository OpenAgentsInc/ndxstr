// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::stream::StreamExt;
use futures::SinkExt;
use mysql::prelude::Queryable;
// use mysql::prelude::TextQuery;
use mysql::Statement;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

/// Simple tag type for array of array of strings.
type Tag = Vec<Vec<String>>;

/// Deserializer that ensures we always have a [`Tag`].
fn tag_from_string<'de, D>(deserializer: D) -> Result<Tag, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    #[serde(skip)]
    pub delegated_by: Option<String>,
    pub created_at: u64,
    pub kind: u64,
    #[serde(deserialize_with = "tag_from_string")]
    // NOTE: array-of-arrays may need to be more general than a string container
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
    // Optimization for tag search, built on demand.
    #[serde(skip)]
    pub tagidx: Option<HashMap<char, HashSet<String>>>,
}

#[tauri::command]
async fn build_relay_list() -> Result<Vec<Option<String>>, String> {
    // Get the database URL from the environment variables
    let url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL not found".to_string())?;

    // Create a connection pool and get a connection from it
    let builder = mysql::OptsBuilder::from_opts(mysql::Opts::from_url(&url).unwrap());
    let pool = mysql::Pool::new(builder.ssl_opts(mysql::SslOpts::default())).unwrap();
    let mut conn = pool.get_conn().unwrap();

    // Query the database for relay URLs
    let rows: Result<
        Vec<std::option::Option<std::option::Option<std::string::String>>>,
        std::string::String,
    > = conn
        .query_map(
            "SELECT JSON_EXTRACT(tags, '$[0][1]') FROM events WHERE kind = 10002",
            |row: mysql::Row| row.get::<Option<String>, _>(0),
        )
        .map_err(|err| format!("Failed to fetch relays: {:?}", err));

    // Deduplicate and return the relay URLs
    let mut relays = HashSet::new();
    for row in rows.unwrap() {
        if let Some(url) = row {
            relays.insert(url);
        }
    }
    println!("Relays: {:?}", relays);
    Ok(relays.into_iter().collect())
}

#[tauri::command]
async fn fetch_events_count() -> Result<usize, String> {
    let url = env::var("DATABASE_URL").expect("DATABASE_URL not found");
    let builder = mysql::OptsBuilder::from_opts(mysql::Opts::from_url(&url).unwrap());
    let pool = mysql::Pool::new(builder.ssl_opts(mysql::SslOpts::default())).unwrap();
    let mut conn = pool.get_conn().unwrap();

    let count: Option<usize> = conn
        .query_first("SELECT COUNT(*) FROM events")
        .map_err(|err| format!("Failed to fetch events count: {:?}", err))?;

    Ok(count.unwrap_or(0))
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
async fn index_events(relayurl: String) -> String {
    let url = env::var("DATABASE_URL").expect("DATABASE_URL not found");
    let builder = mysql::OptsBuilder::from_opts(mysql::Opts::from_url(&url).unwrap());
    let pool = mysql::Pool::new(builder.ssl_opts(mysql::SslOpts::default())).unwrap();
    let mut _conn = pool.get_conn().unwrap();
    println!("Successfully connected to PlanetScale!");

    // Parse the relayurl string as a URL
    let url = Url::parse(&relayurl).unwrap();

    // Connect to the WebSocket server
    let (mut ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("Connected to url: {}", relayurl);

    // Send the subscription message
    let subscription_id = "my_subscription";
    let since_timestamp = (chrono::Utc::now() - chrono::Duration::hours(14)).timestamp();
    let filter = json!({
        "kinds": [0, 1, 40, 41, 42, 43, 44, 10002],
        "limit": 5000,
        "since": since_timestamp,
    });
    let message = json!(["REQ", subscription_id, filter]);
    ws_stream
        .send(Message::Text(message.to_string()))
        .await
        .expect("Failed to send message");

    // Receive and process the events from the server
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(msg) => {
                if let Message::Text(text) = msg {
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(event_array) = event.as_array() {
                            if event_array.len() >= 2 && event_array[0] == "EVENT" {
                                if let Ok(event) =
                                    serde_json::from_value::<Event>(event_array[2].clone())
                                {
                                    println!("Received event: {:?}", event);

                                    // Prepare the SQL statement
                                    let stmt = _conn
                                        .prep(
                                            "
                                            INSERT INTO events (id, pubkey, delegated_by, created_at, kind, tags, content, sig)
                                            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                                            ",
                                        )
                                        .unwrap();
                                    // Bind parameters to the statement
                                    let params = (
                                        event.id,
                                        event.pubkey,
                                        event.delegated_by,
                                        event.created_at,
                                        event.kind,
                                        serde_json::to_string(&event.tags).unwrap(),
                                        serde_json::to_string(&event.content).unwrap(),
                                        event.sig,
                                    );
                                    // Execute the statement with the bound parameters
                                    if let Err(err) =
                                        _conn.exec::<usize, &Statement, _>(&stmt, params)
                                    {
                                        eprintln!("Failed to execute statement: {:?}", err);
                                    }
                                } else if let Err(e) =
                                    serde_json::from_value::<Event>(event_array[2].clone())
                                {
                                    eprintln!("Failed to deserialize event: {:?}", text);
                                }
                            }
                        } else {
                            eprintln!("Received non-array event: {:?}", text);
                        }
                    } else {
                        eprintln!("Failed to parse event: {:?}", text);
                    }
                }
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }

    format!("Indexing {}...", &relayurl)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            build_relay_list,
            fetch_events_count,
            index_events
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
