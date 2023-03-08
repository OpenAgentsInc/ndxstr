// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::stream::StreamExt;
use futures::SinkExt;
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

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
async fn index_events(relayurl: String) -> String {
    let url = env::var("DATABASE_URL").expect("DATABASE_URL not found");
    let builder = mysql::OptsBuilder::from_opts(mysql::Opts::from_url(&url).unwrap());
    let pool = mysql::Pool::new(builder.ssl_opts(mysql::SslOpts::default())).unwrap();
    let _conn = pool.get_conn().unwrap();
    println!("Successfully connected to PlanetScale!");

    // Parse the relayurl string as a URL
    let url = Url::parse(&relayurl).unwrap();

    // Connect to the WebSocket server
    let (mut ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("Connected to url: {}", relayurl);

    // Send the subscription message
    let subscription_id = "my_subscription";
    let filter = json!({
        "kinds": [42],
        "limit": 100
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
                    if let Ok((_, _, event)) = serde_json::from_str::<(_, _, Event)>(&text) {
                        println!("Received event: {:?}", event);
                        // TODO: add the event to the database
                    } else {
                        eprintln!("Failed to deserialize event: {:?}", text);
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
        .invoke_handler(tauri::generate_handler![index_events])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
