use eframe::egui;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use std::time::Duration;
use nostr::{nips::nip47::NostrWalletConnectURI, Filter, Keys, Kind, PublicKey};
use nostr_sdk::{Client, SubscribeAutoCloseOptions};
use std::str::FromStr;

use crate::{
    types::{Config, EditableRelay, NostrStatusAppInternal, ProfileMetadata, TimelinePost, AppTab},
    cache_db::{LmdbCache, DB_FOLLOWED, DB_RELAYS, DB_PROFILES, DB_TIMELINE},
    CONFIG_FILE,
    nostr_client::{connect_to_relays_with_nip65, fetch_nip01_profile, fetch_timeline_events}
};

// --- Step 1: キャッシュからデータを読み込む ---
struct CachedData {
    followed_pubkeys: HashSet<PublicKey>,
    nip65_relays: Vec<(String, Option<String>)>,
    profile_metadata: ProfileMetadata,
    timeline_posts: Vec<TimelinePost>,
}

fn load_data_from_cache(
    cache_db: &LmdbCache,
    pubkey_hex: &str,
) -> Result<CachedData, Box<dyn std::error::Error + Send + Sync>> {
    println!("Loading data from cache for pubkey: {pubkey_hex}");

    let followed_cache = cache_db.read_cache::<HashSet<PublicKey>>(DB_FOLLOWED, pubkey_hex)?;
    let nip65_cache =
        cache_db.read_cache::<Vec<(String, Option<String>)>>(DB_RELAYS, pubkey_hex)?;
    let profile_cache = cache_db.read_cache::<ProfileMetadata>(DB_PROFILES, pubkey_hex)?;
    let timeline_cache = cache_db
        .read_cache::<Vec<TimelinePost>>(DB_TIMELINE, pubkey_hex)
        .ok();

    if followed_cache.is_expired() || nip65_cache.is_expired() || profile_cache.is_expired() {
        return Err("Cache expired".into());
    }

    println!("Successfully loaded data from cache.");
    Ok(CachedData {
        followed_pubkeys: followed_cache.data,
        nip65_relays: nip65_cache.data,
        profile_metadata: profile_cache.data,
        timeline_posts: timeline_cache.map_or(Vec::new(), |c| c.data),
    })
}

// --- Step 2: ネットワークから新しいデータを取得 ---
struct FreshData {
    followed_pubkeys: HashSet<PublicKey>,
    timeline_posts: Vec<TimelinePost>,
    log_message: String,
    fetched_nip65_relays: Vec<(String, Option<String>)>,
    profile_metadata: ProfileMetadata,
    profile_json_string: String,
}

async fn fetch_fresh_data_from_network(
    client: &Client,
    keys: &Keys,
    discover_relays: &str,
    default_relays: &str,
    cache_db: &LmdbCache,
) -> Result<FreshData, Box<dyn std::error::Error + Send + Sync>> {
    let pubkey_hex = keys.public_key().to_string();

    println!("Fetching fresh data from network...");

    let (log_message, fetched_nip65_relays) =
        connect_to_relays_with_nip65(client, keys, discover_relays, default_relays).await?;
    cache_db.write_cache(DB_RELAYS, &pubkey_hex, &fetched_nip65_relays)?;

    println!("Fetching NIP-02 contact list...");
    let nip02_filter = Filter::new()
        .authors(vec![keys.public_key()])
        .kind(Kind::ContactList)
        .limit(1);
    let nip02_filter_id = client
        .subscribe(nip02_filter, Some(SubscribeAutoCloseOptions::default()))
        .await?;

    let mut followed_pubkeys = HashSet::new();
    let mut received_nip02 = false;

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(10)) => {}
        _ = async {
            let mut notifications = client.notifications();
            while let Ok(notification) = notifications.recv().await {
                if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::ContactList && event.pubkey == keys.public_key() {
                        for tag in event.tags.iter() { if let Some(nostr::TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() { followed_pubkeys.insert(*public_key); } }
                        received_nip02 = true;
                        break;
                    }
                }
            }
        } => {},
    }
    client.unsubscribe(&nip02_filter_id).await;

    if received_nip02 {
        cache_db.write_cache(DB_FOLLOWED, &pubkey_hex, &followed_pubkeys)?;
    }

    let timeline_posts = fetch_timeline_events(keys, discover_relays, &followed_pubkeys).await?;
    cache_db.write_cache(DB_TIMELINE, &pubkey_hex, &timeline_posts)?;

    let (profile_metadata, profile_json_string) =
        fetch_nip01_profile(client, keys.public_key()).await?;
    cache_db.write_cache(DB_PROFILES, &pubkey_hex, &profile_metadata)?;

    Ok(FreshData {
        followed_pubkeys,
        timeline_posts,
        log_message,
        fetched_nip65_relays,
        profile_metadata,
        profile_json_string,
    })
}


pub fn draw_login_view(
    ui: &mut egui::Ui,
    app_data: &mut NostrStatusAppInternal,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
    runtime_handle: tokio::runtime::Handle,
) {
    let login_heading_text = "ログインまたは登録";
    let secret_key_label_text = "秘密鍵 (nsec):";
    let secret_key_hint_text = "nsec1...";
    let passphrase_label_text = "パスフレーズ:";
    let passphrase_hint_text = "パスワード";
    let confirm_passphrase_label_text = "パスフレーズの確認:";
    let confirm_passphrase_hint_text = "パスワードを再入力";
    let login_button_text = "ログイン";
    let register_button_text = "登録";

    ui.group(|ui| {
        ui.heading(login_heading_text);
        ui.add_space(10.0);
        if Path::new(CONFIG_FILE).exists() {
            // --- ログイン ---
            ui.horizontal(|ui| {
                ui.label(passphrase_label_text);
                ui.add(egui::TextEdit::singleline(&mut app_data.passphrase_input)
                    .password(true)
                    .hint_text(passphrase_hint_text));
            });

            if ui.button(egui::RichText::new(login_button_text).strong()).clicked() && !app_data.is_loading {
                let passphrase = app_data.passphrase_input.clone();
                let cache_db_clone = app_data.cache_db.clone();
                app_data.is_loading = true;
                app_data.should_repaint = true;
                let cloned_app_data_arc = app_data_arc.clone();
                runtime_handle.clone().spawn(async move {
                    let app_data_for_login_logic = cloned_app_data_arc.clone();
                    let login_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async move {
                        let (keys, nwc_uri) = (|| -> Result<_, Box<dyn std::error::Error + Send + Sync>> {
                            let config_str = fs::read_to_string(CONFIG_FILE)?;
                            let config: Config = serde_json::from_str(&config_str)?;
                            let decrypted_bytes = crate::nip49::decrypt(
                                &config.encrypted_secret_key,
                                &passphrase,
                                &config.salt,
                            )?;
                            let keys = Keys::parse(&hex::encode(&decrypted_bytes))?;

                            let nwc_uri = if let Some(encrypted_nwc) = config.encrypted_nwc_uri {
                                let decrypted_nwc_bytes = crate::nip49::decrypt(
                                    &encrypted_nwc,
                                    &passphrase,
                                    &config.salt,
                                )?;
                                let nwc_uri_str = String::from_utf8(decrypted_nwc_bytes)?;
                                Some(NostrWalletConnectURI::from_str(&nwc_uri_str)?)
                            } else {
                                None
                            };
                            Ok((keys, nwc_uri))
                        })()?;

                        if let Some(uri) = nwc_uri {
                            let app_data_for_nwc_task = app_data_for_login_logic.clone();
                            runtime_handle.clone().spawn(async move {
                                if let Err(e) =
                                    super::wallet_view::connect_nwc(uri, app_data_for_nwc_task.clone())
                                        .await
                                {
                                    eprintln!("Failed to connect to NWC: {}", e);
                                    let mut app_data =
                                        app_data_for_nwc_task.lock().unwrap();
                                    app_data.nwc_error = Some(format!("NWC auto-connect failed: {}", e));
                                }
                            });
                        }

                        let client = Client::new(keys.clone());
                        let pubkey_hex = keys.public_key().to_string();
                        let (discover_relays, default_relays) = {
                            let app_data = app_data_for_login_logic.lock().unwrap();
                            (app_data.discover_relays_editor.clone(), app_data.default_relays_editor.clone())
                        };
                        if let Ok(cached_data) = load_data_from_cache(&cache_db_clone, &pubkey_hex) {
                            let mut app_data = app_data_for_login_logic.lock().unwrap();
                            app_data.my_keys = Some(keys.clone());
                            app_data.nostr_client = Some(client.clone());
                            app_data.followed_pubkeys = cached_data.followed_pubkeys;
                            app_data.timeline_posts = cached_data.timeline_posts;
                            app_data.editable_profile = cached_data.profile_metadata;
                            app_data.nip65_relays = cached_data.nip65_relays.into_iter().map(|(url, policy)| {
                                let (read, write) = match policy.as_deref() {
                                    Some("read") => (true, false),
                                    Some("write") => (false, true),
                                    _ => (true, true),
                                };
                                EditableRelay { url, read, write }
                            }).collect();
                            app_data.is_logged_in = true;
                            app_data.is_loading = true;
                        } else {
                            let mut app_data = app_data_for_login_logic.lock().unwrap();
                            app_data.my_keys = Some(keys.clone());
                            app_data.nostr_client = Some(client.clone());
                            app_data.is_logged_in = true;
                            app_data.is_loading = true;
                        }
                        let fresh_data_result = fetch_fresh_data_from_network(&client, &keys, &discover_relays, &default_relays, &cache_db_clone).await;
                        if let Ok(fresh_data) = fresh_data_result {
                            let mut app_data = app_data_for_login_logic.lock().unwrap();
                            app_data.followed_pubkeys = fresh_data.followed_pubkeys;
                            app_data.timeline_posts = fresh_data.timeline_posts;
                            if let Some(pos) = fresh_data.log_message.find("--- 現在接続中のリレー ---") {
                                app_data.connected_relays_display = fresh_data.log_message[pos..].to_string();
                            }
                            app_data.nip65_relays = fresh_data.fetched_nip65_relays.clone().into_iter().map(|(url, policy)| {
                                let (read, write) = match policy.as_deref() {
                                    Some("read") => (true, false),
                                    Some("write") => (false, true),
                                    _ => (true, true),
                                };
                                EditableRelay { url, read, write }
                            }).collect();
                            let my_emojis: std::collections::HashMap<String, String> = fresh_data.profile_metadata.emojis
                                .iter()
                                .map(|emoji_pair| (emoji_pair[0].clone(), emoji_pair[1].clone()))
                                .collect();
                            app_data.my_emojis = my_emojis;
                            app_data.editable_profile = fresh_data.profile_metadata;
                            app_data.nip01_profile_display = fresh_data.profile_json_string;
                            app_data.profile_fetch_status = "Profile loaded.".to_string();

                            // --- Fetch NIP-30/51 Emojis with fallback ---
                            let pubkey = keys.public_key();
                            let nip65_relays = fresh_data.fetched_nip65_relays.clone();
                            let app_data_clone_for_emojis = app_data_for_login_logic.clone();
                            runtime_handle.clone().spawn(async move {
                                println!("Spawning emoji fetch task for kind:30030...");
                                let nip65_urls: Vec<String> = nip65_relays.iter().map(|(url, _)| url.clone()).collect();

                                let mut custom_emojis = crate::emoji_loader::fetch_emoji_sets(&nip65_urls, pubkey).await;

                                if custom_emojis.is_empty() {
                                    println!("No emojis found in NIP-65 relays, trying default relays...");
                                    let default_relays_str = {
                                        let app_data = app_data_clone_for_emojis.lock().unwrap();
                                        app_data.default_relays_editor.clone()
                                    };
                                    let default_relay_urls: Vec<String> = default_relays_str.lines().map(String::from).collect();
                                    custom_emojis = crate::emoji_loader::fetch_emoji_sets(&default_relay_urls, pubkey).await;
                                }

                                if !custom_emojis.is_empty() {
                                    println!("Fetched {} custom emojis from kind:30030.", custom_emojis.len());
                                    let mut app_data = app_data_clone_for_emojis.lock().unwrap();
                                    app_data.my_emojis.extend(custom_emojis);
                                    app_data.should_repaint = true;
                                } else {
                                    println!("No custom emojis found from NIP-65 or default relays.");
                                }
                            });
                            // --- End Fetch Emojis ---
                        } else if let Err(e) = fresh_data_result {
                            let mut app_data = app_data_for_login_logic.lock().unwrap();
                            app_data.profile_fetch_status = format!("Failed to refresh data: {e}");
                        }
                        Ok(())
                    }.await;
                    if let Err(e) = login_result {
                        let client_to_shutdown = {
                            let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                            app_data_in_task.nostr_client.take()
                        };
                        if let Some(client) = client_to_shutdown { client.shutdown().await; }
                        let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                        app_data_in_task.profile_fetch_status = format!("Login failed: {e}");
                    }
                    let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                    app_data_in_task.is_loading = false;
                    app_data_in_task.should_repaint = true;
                });
            }
        } else {
            // --- 新規登録 ---
            ui.horizontal(|ui| {
                ui.label(secret_key_label_text);
                ui.add(egui::TextEdit::singleline(&mut app_data.secret_key_input)
                    .password(true)
                    .hint_text(secret_key_hint_text));
            });

            ui.horizontal(|ui| {
                ui.label(passphrase_label_text);
                ui.add(egui::TextEdit::singleline(&mut app_data.passphrase_input)
                    .password(true)
                    .hint_text(passphrase_hint_text));
            });
            ui.horizontal(|ui| {
                ui.label(confirm_passphrase_label_text);
                ui.add(egui::TextEdit::singleline(&mut app_data.confirm_passphrase_input)
                    .password(true)
                    .hint_text(confirm_passphrase_hint_text));
            });

            if ui.button(egui::RichText::new(register_button_text).strong()).clicked() && !app_data.is_loading {
                let secret_key_input = app_data.secret_key_input.clone();
                let passphrase = app_data.passphrase_input.clone();
                let confirm_passphrase = app_data.confirm_passphrase_input.clone();
                let cache_db_clone = app_data.cache_db.clone();
                app_data.is_loading = true;
                app_data.should_repaint = true;
                let cloned_app_data_arc = app_data_arc.clone();
                runtime_handle.clone().spawn(async move {
                    if passphrase != confirm_passphrase {
                        let mut current_app_data = cloned_app_data_arc.lock().unwrap();
                        current_app_data.profile_fetch_status = "Passphrases do not match.".to_string();
                        current_app_data.is_loading = false;
                        current_app_data.should_repaint = true;
                        return;
                    }
                    let registration_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                        let keys = (|| -> Result<Keys, Box<dyn std::error::Error + Send + Sync>> {
                            let user_provided_keys = Keys::parse(&secret_key_input)?;
                            let plaintext_bytes = user_provided_keys.secret_key().to_secret_bytes();
                            let (nip49_encoded, salt_base64) =
                                crate::nip49::encrypt(&plaintext_bytes, &passphrase)?;
                            let config = Config {
                                encrypted_secret_key: nip49_encoded,
                                salt: salt_base64,
                                encrypted_nwc_uri: None,
                            };
                            let config_json = serde_json::to_string_pretty(&config)?;
                            fs::write(CONFIG_FILE, config_json)?;
                            Ok(user_provided_keys)
                        })()?;
                        let client = Client::new(keys.clone());
                        let (discover_relays, default_relays) = {
                            let app_data = cloned_app_data_arc.lock().unwrap();
                            (app_data.discover_relays_editor.clone(), app_data.default_relays_editor.clone())
                        };
                        let fresh_data_result = fetch_fresh_data_from_network(&client, &keys, &discover_relays, &default_relays, &cache_db_clone).await;
                        if let Ok(fresh_data) = fresh_data_result {
                            let mut app_data = cloned_app_data_arc.lock().unwrap();
                            app_data.my_keys = Some(keys.clone());
                            app_data.nostr_client = Some(client);
                            app_data.is_logged_in = true;
                            app_data.current_tab = AppTab::Home;
                            app_data.followed_pubkeys = fresh_data.followed_pubkeys;
                            app_data.timeline_posts = fresh_data.timeline_posts;
                            if let Some(pos) = fresh_data.log_message.find("--- 現在接続中のリレー ---") {
                                app_data.connected_relays_display = fresh_data.log_message[pos..].to_string();
                            }
                            app_data.nip65_relays = fresh_data.fetched_nip65_relays.clone().into_iter().map(|(url, policy)| {
                                let (read, write) = match policy.as_deref() {
                                    Some("read") => (true, false),
                                    Some("write") => (false, true),
                                    _ => (true, true),
                                };
                                EditableRelay { url, read, write }
                            }).collect();
                            let my_emojis: std::collections::HashMap<String, String> = fresh_data.profile_metadata.emojis
                                .iter()
                                .map(|emoji_pair| (emoji_pair[0].clone(), emoji_pair[1].clone()))
                                .collect();
                            app_data.my_emojis = my_emojis;
                            app_data.editable_profile = fresh_data.profile_metadata;
                            app_data.nip01_profile_display = fresh_data.profile_json_string;
                            app_data.profile_fetch_status = "Profile loaded.".to_string();

                            // --- Fetch NIP-30/51 Emojis with fallback ---
                            let pubkey = keys.public_key();
                            let nip65_relays = fresh_data.fetched_nip65_relays.clone();
                            let app_data_clone_for_emojis = cloned_app_data_arc.clone();
                            runtime_handle.clone().spawn(async move {
                                println!("Spawning emoji fetch task for kind:30030...");
                                let nip65_urls: Vec<String> = nip65_relays.iter().map(|(url, _)| url.clone()).collect();

                                let mut custom_emojis = crate::emoji_loader::fetch_emoji_sets(&nip65_urls, pubkey).await;

                                if custom_emojis.is_empty() {
                                    println!("No emojis found in NIP-65 relays, trying default relays...");
                                    let default_relays_str = {
                                        let app_data = app_data_clone_for_emojis.lock().unwrap();
                                        app_data.default_relays_editor.clone()
                                    };
                                    let default_relay_urls: Vec<String> = default_relays_str.lines().map(String::from).collect();
                                    custom_emojis = crate::emoji_loader::fetch_emoji_sets(&default_relay_urls, pubkey).await;
                                }

                                if !custom_emojis.is_empty() {
                                    println!("Fetched {} custom emojis from kind:30030.", custom_emojis.len());
                                    let mut app_data = app_data_clone_for_emojis.lock().unwrap();
                                    app_data.my_emojis.extend(custom_emojis);
                                    app_data.should_repaint = true;
                                } else {
                                    println!("No custom emojis found from NIP-65 or default relays.");
                                }
                            });
                            // --- End Fetch Emojis ---
                        } else if let Err(e) = fresh_data_result {
                            eprintln!("Failed to fetch initial data for registration: {e}");
                        }
                        Ok(())
                    }.await;
                    if let Err(e) = registration_result {
                        eprintln!("Failed to register new key: {e}");
                        let client_to_shutdown = {
                            let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                            app_data_in_task.nostr_client.take()
                        };
                        if let Some(client) = client_to_shutdown { client.shutdown().await; }
                    }
                    let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                    app_data_async.is_loading = false;
                    app_data_async.should_repaint = true;
                });
            }
        }
    });
}
