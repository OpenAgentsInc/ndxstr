// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::stream::StreamExt;
use futures::SinkExt;
use mysql::prelude::Queryable;
use mysql::Statement;
use postgres::Config;
use postgres::{Client, NoTls};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use tauri::Window;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(serde::Serialize)]
struct RelayConnectionStatus {
    url: String,
    connected: bool,
}

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
async fn move_events() -> Result<String, String> {
    // Get the database URL from the environment variable
    let connection_string =
        env::var("FORGE_DATABASE_URL").map_err(|_| "FORGE_DATABASE_URL not found".to_string())?;

    println!("Connection string set...");

    // Parse the database URL to get the host and port
    let url = url::Url::parse(&connection_string).map_err(|err| err.to_string())?;
    let host = url
        .host_str()
        .ok_or_else(|| "Invalid database URL".to_string())?;
    let port = url.port().unwrap_or(5432);

    // Set up an SSH tunnel to the remote server
    let ssh_key_path = Path::new("/Users/christopherdavid/.ssh/id_ed25519");
    let ssh_username = "forge";
    let ssh_host = "127.0.0.1";
    let ssh_port = 22;
    let local_port = 5432; // the local port to forward to

    println!("ssh infos set");

    let tcp_stream = TcpStream::connect((ssh_host, ssh_port)).map_err(|err| err.to_string())?;
    println!("1");
    let mut ssh_session = ssh2::Session::new().unwrap();
    println!("2");
    ssh_session.set_tcp_stream(tcp_stream);
    println!("3");
    ssh_session.handshake().unwrap();
    println!("4");
    ssh_session
        .userauth_pubkey_file(ssh_username, None, ssh_key_path, None)
        .map_err(|err| err.to_string())?;

    println!("tcpstream something maybe...");

    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", local_port)).map_err(|err| err.to_string())?;

    println!("listener set...");

    let mut stream = ssh_session
        .channel_direct_tcpip(host, port, None)
        .map_err(|err| err.to_string())?;

    let mut client_stream = listener.accept().map_err(|err| err.to_string())?.0;
    std::thread::spawn(move || {
        std::io::copy(&mut stream, &mut client_stream).unwrap();
    });

    // Configure the PostgreSQL client
    let mut config = Config::new();
    config.host("127.0.0.1");
    config.port(local_port);
    config.user(url.username());
    config.password(url.password().unwrap_or(""));
    config.dbname(url.path().trim_start_matches("/"));

    // Connect to the database
    let mut client = config.connect(NoTls).map_err(|err| err.to_string())?;
    println!("Connected maybe?");

    // Use the client to execute SQL queries
    let rows = client
        .query("SELECT * FROM channels", &[])
        .map_err(|err| err.to_string())?;
    for row in rows {
        let value: i32 = row.get(0);
        println!("Value: {}", value);
    }

    Ok("Hello from Rust".to_string())
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
async fn index_events(relayurl: String, window: Window) -> String {
    let url = env::var("DATABASE_URL").expect("DATABASE_URL not found");
    let builder = mysql::OptsBuilder::from_opts(mysql::Opts::from_url(&url).unwrap());
    let pool = mysql::Pool::new(builder.ssl_opts(mysql::SslOpts::default())).unwrap();
    let mut _conn = pool.get_conn().unwrap();
    println!("Successfully connected to PlanetScale!");

    // Parse the relayurl string as a URL
    let url = Url::parse(&relayurl).unwrap();

    // Connect to the WebSocket server
    // let (mut ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    // println!("Connected to url: {}", relayurl);

    // Connect to the WebSocket server
    let result = connect_async(url).await;
    let mut ws_stream = match result {
        Ok((ws_stream, _)) => {
            println!("Successfully connected to WebSocket server: {}", relayurl);
            let status = json!({ "relayUrl": relayurl, "status": "connected" });
            window
                .emit("relay-connection-change", status.to_string())
                .unwrap();
            ws_stream
        }
        Err(err) => {
            let status = json!({ "relayUrl": relayurl, "status": "notconnected" });
            window
                .emit("relay-connection-change", status.to_string())
                .unwrap();
            eprintln!("Failed to connect to WebSocket server: {}", err);
            return format!("Failed to connect to WebSocket server: {}", err);
        }
    };

    // window
    //     .emit(
    //         "relay-connection-change",
    //         format!("Connected to url: {}", relayurl),
    //     )
    //     .unwrap();

    // Send the subscription message
    let subscription_id = "my_subscription";
    let since_timestamp = (chrono::Utc::now() - chrono::Duration::weeks(8)).timestamp();
    let filter = json!({
        "kinds": [0, 40, 41, 42, 43, 44, 9734, 9735, 10002],
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
                                    // println!("Received event: {:?}", event.id);
                                    // log received event kind and id
                                    println!("Received event: {:?}", event.kind);
                                    // println!("Received event: {:?}", event);
                                    // window.emit("got-an-event", "Got an event.").unwrap();

                                    // Prepare the SQL statement
                                    let stmt = _conn
                                    .prep(
                                        "
                                        INSERT INTO events (id, pubkey, delegated_by, created_at, kind, tags, content, sig, relayurl)
                                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                                        &relayurl,
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
                let status = json!({ "relayUrl": relayurl, "status": "disconnected" });
                window
                    .emit("relay-connection-change", status.to_string())
                    .unwrap();
                // Emit an event when the connection is closed
                break;
            }
        }
    }

    format!("Indexing {}...", relayurl)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            build_relay_list,
            fetch_events_count,
            index_events,
            move_events
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
