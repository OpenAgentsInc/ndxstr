// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::stream::StreamExt;
use futures::SinkExt;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
async fn index_events(relayurl: String) -> String {
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

    // Receive and print the events from the server
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(msg) => println!("Received message: {:?}", msg),
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
