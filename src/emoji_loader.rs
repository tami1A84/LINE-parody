use async_tungstenite::tokio::connect_async;
use async_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use nostr::PublicKey;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;

#[derive(Deserialize, Debug)]
pub struct RawNostrEvent {
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventPointer {
    pub pubkey: PublicKey,
    pub d_identifier: String,
}

pub async fn fetch_emoji_sets(
    relays: &[String],
    pubkey: PublicKey,
) -> HashMap<String, String> {
    let mut all_emojis = HashMap::new();
    let pk_hex = pubkey.to_string();

    // Stage 1: Fetch initial lists and pointers
    let mut all_pointers = HashSet::new();

    let stage1_futures = relays.iter().map(|url| {
        let url = url.clone();
        let pk_hex_clone = pk_hex.clone();
        async move {
            timeout(
                Duration::from_secs(10),
                fetch_from_relay(&url, Some(&pk_hex_clone), None)
            ).await
        }
    });

    let stage1_results = futures_util::future::join_all(stage1_futures).await;

    for result in stage1_results {
        if let Ok(Ok((emojis, pointers))) = result {
            all_emojis.extend(emojis);
            all_pointers.extend(pointers);
        }
    }

    // Stage 2: Fetch emojis from pointers if any were found
    if !all_pointers.is_empty() {
        println!("Found {} emoji set pointers. Fetching referenced sets...", all_pointers.len());

        let authors: Vec<String> = all_pointers.iter().map(|p| p.pubkey.to_string()).collect();
        let d_tags: Vec<String> = all_pointers.iter().map(|p| p.d_identifier.clone()).collect();

        let secondary_filter = json!({
            "authors": authors,
            "kinds": [30030],
            "#d": d_tags
        });

        let stage2_futures = relays.iter().map(|url| {
            let url = url.clone();
            let filter = secondary_filter.clone();
            async move {
                timeout(
                    Duration::from_secs(10),
                    fetch_from_relay(&url, None, Some(filter))
                ).await
            }
        });

        let stage2_results = futures_util::future::join_all(stage2_futures).await;

        for result in stage2_results {
            if let Ok(Ok((emojis, _))) = result { // We don't care about pointers from the second stage
                all_emojis.extend(emojis);
            }
        }
    }

    all_emojis
}

pub async fn fetch_from_relay(
    url: &str,
    primary_pubkey_hex: Option<&str>,
    secondary_filter: Option<serde_json::Value>,
) -> Result<(HashMap<String, String>, Vec<EventPointer>), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async(url).await.map_err(|e| format!("Connection to {} failed: {}", url, e))?;
    let (mut write, mut read) = ws_stream.split();

    let sub_id = format!("emoji-fetch-{}", rand::random::<u32>());

    let filter = secondary_filter.unwrap_or_else(|| json!({
        "authors": [primary_pubkey_hex.unwrap_or_default()],
        "kinds": [30030, 10030],
    }));

    let request = json!(["REQ", sub_id, filter]);
    write
        .send(Message::Text(request.to_string()))
        .await?;

    let mut emojis = HashMap::new();
    let mut pointers = Vec::new();

    let read_loop = async {
        while let Some(message_result) = read.next().await {
            let message = match message_result {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error reading message from {}: {}", url, e);
                    break;
                }
            };

            if let Message::Text(text) = message {
                let parsed: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if let Some(msg_type) = parsed.get(0).and_then(|v| v.as_str()) {
                    match msg_type {
                        "EVENT" => {
                            if let (Some(sid), Some(event_json)) = (parsed.get(1).and_then(|v| v.as_str()), parsed.get(2)) {
                                if sid == sub_id {
                                    if let Ok(event) = serde_json::from_value::<RawNostrEvent>(event_json.clone()) {
                                        let mut is_emoji_list = false;
                                        let mut is_pointer_list = false;

                                        if event.kind == 30030 {
                                            is_emoji_list = true;
                                        } else if event.kind == 10030 {
                                            if event.tags.iter().any(|t| t.get(0).map_or(false, |v| v == "d") && t.get(1).map_or(false, |v| v == "emojis")) {
                                                is_emoji_list = true;
                                            } else if event.tags.iter().any(|t| t.get(0).map_or(false, |v| v == "a")) {
                                                is_pointer_list = true;
                                            }
                                        }

                                        if is_emoji_list {
                                            for tag in &event.tags {
                                                if tag.len() >= 3 && tag[0] == "emoji" {
                                                    let shortcode = &tag[1];
                                                    let image_url = tag[2].clone();
                                                    let shortcode_key = shortcode.trim_matches(':').to_string();
                                                    if !shortcode_key.is_empty() {
                                                        emojis.insert(shortcode_key, image_url);
                                                    }
                                                }
                                            }
                                        }

                                        if is_pointer_list {
                                            for tag in &event.tags {
                                                if tag.len() >= 2 && tag[0] == "a" {
                                                    let parts: Vec<&str> = tag[1].split(':').collect();
                                                    if parts.len() == 3 && parts[0] == "30030" {
                                                        if let Ok(pubkey) = PublicKey::from_str(parts[1]) {
                                                            let d_identifier = parts[2].to_string();
                                                            pointers.push(EventPointer { pubkey, d_identifier });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "EOSE" => {
                            if let Some(sid) = parsed.get(1).and_then(|v| v.as_str()) {
                                if sid == sub_id { break; }
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
    };

    if timeout(Duration::from_secs(10), read_loop).await.is_err() {
        eprintln!("Timeout while waiting for messages from {}", url);
    }

    let close_message = json!(["CLOSE", sub_id]);
    let _ = write.send(Message::Text(close_message.to_string())).await;
    let _ = write.close().await;

    Ok((emojis, pointers))
}
