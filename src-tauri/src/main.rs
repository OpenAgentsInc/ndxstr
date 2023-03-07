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
        "kinds": [40]
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

// #[tauri::command]
// async fn index_events(relayurl: String) -> String {
//     format!("Indexing {}...", relayurl);

//     // Open a WebSocket connection to the relay
//     let url = Url::parse(&relayurl).unwrap();
//     let port = url.port().unwrap_or(80);
//     let addrs = url.socket_addrs(|| Some(port)).unwrap();
//     let stream = TcpStream::connect(&addrs[..]).await.unwrap();
//     let ws = WebSocket::from_raw_socket(stream, Role::Client, None)
//         .await
//         .unwrap();
//     let mut ws_stream = WebSocketStream::new(ws);

//     // Send a test message to the relay
//     ws_stream
//         .send(Message::Text("Hello, relay!".into()))
//         .await
//         .unwrap();

//     // Receive messages from the relay
//     while let Some(message) = ws_stream.next().await {
//         match message.unwrap() {
//             Message::Text(text) => {
//                 println!("Received text message from relay: {}", text);
//             }
//             Message::Binary(_) => {
//                 println!("Received binary message from relay");
//             }
//             Message::Ping(_) => {
//                 println!("Received ping message from relay");
//             }
//             Message::Pong(_) => {
//                 println!("Received pong message from relay");
//             }
//             Message::Close(_) => {
//                 println!("Received close message from relay");
//                 break;
//             }
//         }
//     }

//     "WebSocket connection closed".to_string()
// }

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![index_events])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
