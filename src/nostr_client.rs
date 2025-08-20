use futures::future::join_all;
use nostr::{Filter, Keys, Kind, PublicKey, Tag as NostrTag, nips::nip19::ToBech32};
use nostr_sdk::{Client, ClientOptions as Options, SubscribeAutoCloseOptions};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::types::{ProfileMetadata, TimelinePost};

// NIP-65とフォールバックを考慮したリレー接続関数
pub async fn connect_to_relays_with_nip65(
    client: &Client,
    keys: &Keys,
    discover_relays_str: &str,
    default_relays_str: &str,
) -> Result<(String, Vec<(String, Option<String>)>), Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap_relays: Vec<String> =
        discover_relays_str.lines().map(|s| s.to_string()).collect();

    let client_opts = Options::new();
    let discover_client = Client::builder()
        .signer(keys.clone())
        .opts(client_opts)
        .build();
    discover_client.connect().await;
    discover_client
        .wait_for_connection(Duration::from_secs(30))
        .await;

    let mut status_log = String::new();
    status_log.push_str("NIP-65リレーリストを取得するためにDiscoverリレーに並列接続中...\n");

    let add_relay_futures = bootstrap_relays.iter().map(|url| {
        let discover_client = &discover_client;
        let url = url.clone();
        async move { discover_client.add_relay(url.clone()).await.map(|_| url) }
    });

    let results = join_all(add_relay_futures).await;
    for (i, result) in results.into_iter().enumerate() {
        let url = &bootstrap_relays[i];
        match result {
            Ok(_) => status_log.push_str(&format!("  Discoverリレー追加: {url}\n")),
            Err(e) => {
                status_log.push_str(&format!("  Discoverリレー追加失敗: {url} - エラー: {e}\n"))
            }
        }
    }

    discover_client.connect().await; // Connect discover_client
    tokio::time::sleep(Duration::from_secs(2)).await; // Discoverリレー接続安定待ち

    let filter = Filter::new()
        .authors(vec![keys.public_key()])
        .kind(Kind::RelayList);

    status_log.push_str("NIP-65リレーリストイベントを検索中 (最大10秒)..\n"); // Timeout reduced
    let timeout_filter_id = discover_client
        .subscribe(filter, Some(SubscribeAutoCloseOptions::default()))
        .await?;

    let mut nip65_relays: Vec<(String, Option<String>)> = Vec::new();
    let mut received_nip65_event = false;

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(10)) => { // Timeout reduced
            status_log.push_str("NIP-65イベント検索タイムアウト。\n");
        }
        _ = async {
            let mut notifications = discover_client.notifications();
            while let Ok(notification) = notifications.recv().await {
                if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::RelayList && event.pubkey == keys.public_key() {
                        status_log.push_str("NIP-65リレーリストイベントを受信しました。\n");
                        for tag in event.tags.iter() {
                            let tag_vec = tag.clone().to_vec();
                            if tag_vec.get(0).map(|s| s.as_str()) == Some("r") {
                                if let Some(url) = tag_vec.get(1) {
                                    let policy = tag_vec.get(2).cloned();
                                    nip65_relays.push((url.clone(), policy));
                                }
                            }
                        }
                        received_nip65_event = true;
                        break;
                    }
                }
            }
        } => {}
    }

    discover_client.unsubscribe(&timeout_filter_id).await;
    discover_client.shutdown().await;

    status_log.push_str("--- NIP-65で受信したリレー情報 ---\n");
    if nip65_relays.is_empty() {
        status_log.push_str("  有効なNIP-65リレーは受信しませんでした。\n");
    } else {
        for (url, policy) in &nip65_relays {
            status_log.push_str(&format!("  URL: {url}, Policy: {policy:?}\n"));
        }
    }
    status_log.push_str("---------------------------------\n");

    let mut current_connected_relays = Vec::new();
    let mut connected_relays_map: std::collections::HashMap<String, nostr_sdk::RelayStatus> =
        std::collections::HashMap::new();

    if received_nip65_event && !nip65_relays.is_empty() {
        status_log.push_str("\nNIP-65で検出されたリレーに並列接続中...\n");
        let _ = client.remove_all_relays().await;

        let relays_to_add: Vec<_> = nip65_relays
            .iter()
            .filter(|(_, policy)| policy.as_deref() == Some("write") || policy.is_none())
            .map(|(url, _)| url.clone())
            .collect();

        let add_relay_futures = relays_to_add.iter().map(|url| {
            let client = &client;
            let url = url.clone();
            async move { client.add_relay(url.clone()).await.map(|_| url) }
        });

        let results = join_all(add_relay_futures).await;
        for result in results {
            match result {
                Ok(url) => status_log.push_str(&format!("  リレー追加: {url}\n")),
                Err(e) => status_log.push_str(&format!("  リレー追加失敗 - エラー: {e}\n")), // URL might not be available on error
            }
        }
    } else {
        status_log.push_str(
            "\nNIP-65リレーリストが見つからなかったため、デフォルトのリレーに並列接続します。\n",
        );
        let _ = client.remove_all_relays().await;

        let fallback_relays: Vec<String> = default_relays_str
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let add_relay_futures = fallback_relays.iter().map(|url| {
            let client = &client;
            let url = url.clone();
            async move { client.add_relay(url.clone()).await.map(|_| url) }
        });

        let results = join_all(add_relay_futures).await;
        for (i, result) in results.into_iter().enumerate() {
            let url = &fallback_relays[i];
            match result {
                Ok(_) => status_log.push_str(&format!("  デフォルトリレー追加: {url}\n")),
                Err(e) => status_log.push_str(&format!(
                    "  デフォルトリレー追加失敗: {url} - エラー: {e}\n"
                )),
            }
        }
    }

    client.connect().await;
    tokio::time::sleep(Duration::from_secs(2)).await; // 接続安定待ち

    let relays = client.relays().await;
    if relays.is_empty() {
        return Err("接続できるリレーがありません。".into());
    }

    status_log.push_str(&format!(
        "\n--- 現在接続中のリレー ({}件) ---\n",
        relays.len()
    ));
    for (url, relay) in relays.iter() {
        let status = relay.status();
        status_log.push_str(&format!("  - {url}: {status:?}\n"));
        current_connected_relays.push(format!("- {url}: {status:?}"));
        connected_relays_map.insert(url.to_string(), status);
    }
    status_log.push_str("---------------------------------\n");

    let full_log = format!(
        "{}\n\n--- 現在接続中のリレー ---\n{}",
        status_log,
        current_connected_relays.join("\n")
    );
    Ok((full_log, nip65_relays))
}

// フォローしているユーザーのリレーリスト(kind:10002)を取得する関数
pub async fn fetch_relays_for_followed_users(
    discover_client: &Client,
    pubkeys: Vec<PublicKey>,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    let filter = Filter::new().authors(pubkeys).kind(Kind::RelayList);

    let events = discover_client
        .fetch_events(filter, Duration::from_secs(10))
        .await?;

    let mut relay_urls = std::collections::HashSet::new();
    for event in events {
        for tag in event.tags.iter() {
            let tag_parts = tag.clone().to_vec();
            if tag_parts.get(0).map(|s| s.as_str()) == Some("r") {
                if let Some(url) = tag_parts.get(1) {
                    relay_urls.insert(url.clone());
                }
            }
        }
    }

    Ok(relay_urls.into_iter().collect())
}

// NIP-01 プロファイルメタデータを取得する関数
pub async fn fetch_nip01_profile(
    client: &Client,
    public_key: PublicKey,
) -> Result<(ProfileMetadata, String), Box<dyn std::error::Error + Send + Sync>> {
    let nip01_filter = Filter::new()
        .authors(vec![public_key])
        .kind(Kind::Metadata)
        .limit(1);
    let nip01_filter_id = client
        .subscribe(nip01_filter, Some(SubscribeAutoCloseOptions::default()))
        .await?;

    let mut profile_json_string = String::new();
    let mut received_nip01 = false;

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            eprintln!("NIP-01 profile fetch timed out.");
        }
        _ = async {
            let mut notifications = client.notifications();
            while let Ok(notification) = notifications.recv().await {
                if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::Metadata && event.pubkey == public_key {
                        profile_json_string = event.content.clone();
                        received_nip01 = true;
                        break;
                    }
                }
            }
        } => {},
    }
    client.unsubscribe(&nip01_filter_id).await;

    if received_nip01 {
        let profile_metadata: ProfileMetadata = serde_json::from_str(&profile_json_string)?;
        Ok((profile_metadata, profile_json_string))
    } else {
        let default_metadata = ProfileMetadata::default();
        let default_json = serde_json::to_string_pretty(&default_metadata)?;
        Ok((default_metadata, default_json)) // プロファイルが見つからなかった場合はデフォルト値を返す
    }
}

// NIP-02 コンタクトリストを更新する関数
pub async fn update_contact_list(
    client: &Client,
    keys: &Keys,
    pubkey_to_modify: PublicKey,
    follow: bool, // trueでフォロー、falseでアンフォロー
) -> Result<HashSet<PublicKey>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. 現在のコンタクトリストを取得
    let filter = Filter::new()
        .authors(vec![keys.public_key()])
        .kind(Kind::ContactList)
        .limit(1);
    let events = client.fetch_events(filter, Duration::from_secs(10)).await?;

    let mut current_tags: Vec<NostrTag> = if let Some(event) = events.first() {
        event.tags.clone().into_iter().collect()
    } else {
        Vec::new()
    };

    let mut followed_pubkeys: HashSet<PublicKey> = current_tags
        .iter()
        .filter_map(|tag| {
            if let Some(nostr::TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                Some(*public_key)
            } else {
                None
            }
        })
        .collect();

    // 2. フォローリストを変更
    if follow {
        if followed_pubkeys.insert(pubkey_to_modify) {
            current_tags.push(NostrTag::public_key(pubkey_to_modify));
            println!("Following {}", pubkey_to_modify.to_bech32()?);
        }
    } else if followed_pubkeys.remove(&pubkey_to_modify) {
        current_tags.retain(|tag| {
            if let Some(nostr::TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                *public_key != pubkey_to_modify
            } else {
                true
            }
        });
        println!("Unfollowing {}", pubkey_to_modify.to_bech32()?);
    }

    // 3. 新しいコンタクトリストイベントを作成して送信
    use nostr::EventBuilder;
    let event = EventBuilder::new(Kind::ContactList, "")
        .tags(current_tags)
        .sign(keys)
        .await?;
    client.send_event(&event).await?;

    println!("Contact list updated successfully.");

    Ok(followed_pubkeys)
}

pub async fn get_profile_metadata(
    pubkey: PublicKey,
    client: &Client,
) -> Result<ProfileMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let filter = Filter::new().authors(vec![pubkey]).kind(Kind::Metadata).limit(1);

    // Get relays from client
    let relays = client.relays().await;
    let relay_urls: Vec<String> = relays.keys().map(|url| url.to_string()).collect();

    let events = client.fetch_events_from(relay_urls, filter, Duration::from_secs(5)).await?;

    if let Some(event) = events.first() {
        let metadata: ProfileMetadata = serde_json::from_str(&event.content)?;
        Ok(metadata)
    } else {
        // Return a default profile if none is found
        Ok(ProfileMetadata::default())
    }
}

pub async fn fetch_timeline_events(
    keys: &Keys,
    discover_relays: &str,
    followed_pubkeys: &HashSet<PublicKey>,
) -> Result<Vec<TimelinePost>, Box<dyn std::error::Error + Send + Sync>> {
    let mut timeline_posts = Vec::new();
    if followed_pubkeys.is_empty() {
        return Ok(timeline_posts);
    }

    let temp_discover_client = Client::new(keys.clone());
    for relay_url in discover_relays.lines().filter(|url| !url.trim().is_empty()) {
        temp_discover_client.add_relay(relay_url.trim()).await?;
    }
    temp_discover_client.connect().await;
    let followed_pubkeys_vec: Vec<PublicKey> = followed_pubkeys.iter().cloned().collect();
    let write_relay_urls =
        fetch_relays_for_followed_users(&temp_discover_client, followed_pubkeys_vec).await?;
    temp_discover_client.shutdown().await;

    if !write_relay_urls.is_empty() {
        let temp_fetch_client = Client::new(keys.clone());
        for url in &write_relay_urls {
            temp_fetch_client.add_relay(url.clone()).await?;
        }
        temp_fetch_client.connect().await;

        let timeline_filter = Filter::new()
            .authors(followed_pubkeys.clone())
            .kind(Kind::from(30315))
            .limit(20);
        let status_events = temp_fetch_client
            .fetch_events(timeline_filter, Duration::from_secs(10))
            .await?;

        if !status_events.is_empty() {
            let author_pubkeys: HashSet<PublicKey> =
                status_events.iter().map(|e| e.pubkey).collect();
            let metadata_filter = Filter::new()
                .authors(author_pubkeys.into_iter())
                .kind(Kind::Metadata);
            let metadata_events = temp_fetch_client
                .fetch_events(metadata_filter, Duration::from_secs(5))
                .await?;
            let mut profiles: HashMap<PublicKey, ProfileMetadata> = HashMap::new();
            for event in metadata_events {
                if let Ok(metadata) = serde_json::from_str::<ProfileMetadata>(&event.content) {
                    profiles.insert(event.pubkey, metadata);
                }
            }

            for event in status_events {
                let emojis = event
                    .tags
                    .iter()
                    .filter_map(|tag| {
                        if let Some(nostr::TagStandard::Emoji { shortcode, url }) =
                            tag.as_standardized()
                        {
                            Some((shortcode.to_string(), url.to_string()))
                        } else {
                            None
                        }
                    })
                    .collect();

                timeline_posts.push(TimelinePost {
                    id: event.id,
                    kind: event.kind,
                    author_pubkey: event.pubkey,
                    author_metadata: profiles.get(&event.pubkey).cloned().unwrap_or_default(),
                    content: event.content.clone(),
                    created_at: event.created_at,
                    emojis,
                    tags: event.tags.to_vec(),
                });
            }
            timeline_posts.sort_by_key(|p| std::cmp::Reverse(p.created_at));
        }
        temp_fetch_client.shutdown().await;
    }
    Ok(timeline_posts)
}