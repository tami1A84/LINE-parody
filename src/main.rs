use eframe::egui;
use nostr::{EventBuilder, Filter, Kind, Keys, PublicKey, Tag};
use nostr_sdk::{Client, Options, SubscribeAutoCloseOptions};
use std::time::Duration;
use nostr::nips::nip19::ToBech32;

use std::fs;
use std::path::Path;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

// NIP-49 (ChaCha20Poly1305) のための暗号クレート
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce, Key,
};
use rand::Rng;
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose};
use hex;

// PBKDF2のためのクレート
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

// serde を使って設定ファイルを構造体として定義
use serde::{Serialize, Deserialize};

const CONFIG_FILE: &str = "config.json"; // 設定ファイル名
const MAX_STATUS_LENGTH: usize = 140; // ステータス最大文字数

#[derive(Serialize, Deserialize)]
struct Config {
    encrypted_secret_key: String, // NIP-49フォーマットの暗号化された秘密鍵
    salt: String, // PBKDF2に使用するソルト (Base64エンコード)
}

// アプリケーションの内部状態を保持する構造体
pub struct NostrStatusAppInternal {
    pub is_logged_in: bool,
    pub status_message_input: String, // ユーザーが入力するステータス
    pub secret_key_input: String, // 初回起動時の秘密鍵入力用
    pub passphrase_input: String,
    pub confirm_passphrase_input: String,
    pub nostr_client: Option<Client>,
    pub my_keys: Option<Keys>,
    pub followed_pubkeys: HashSet<PublicKey>, // NIP-02で取得したフォローリスト
    pub followed_pubkeys_display: String, // GUI表示用の文字列
    pub status_timeline_display: String, // GUI表示用のステータスタイムライン
    pub should_repaint: bool, // UIの再描画をトリガーするためのフラグ
    pub is_loading: bool, // 処理中であることを示すフラグ
    pub current_tab: AppTab, // 現在選択されているタブ
    pub connected_relays_display: String, // 接続中のリレー表示用
}

// タブの状態を管理するenum
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum AppTab {
    Home, // ログイン/登録画面とタイムラインを含む
    Relays,
    Profile,
}

// eframe::Appトレイトを実装する構造体
pub struct NostrStatusApp {
    data: Arc<Mutex<NostrStatusAppInternal>>,
    runtime: Runtime, // Tokio Runtimeを保持
}

impl NostrStatusApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");

        // egui のスタイル設定
        _cc.egui_ctx.set_pixels_per_point(1.2); // UIのスケールを調整
        let mut style = (*_cc.egui_ctx.style()).clone();
        
        // --- クラシックなデザインのためのスタイル調整 ---
        style.visuals = egui::Visuals::light(); 

        let classic_gray_background = egui::Color32::from_rgb(220, 220, 220); 
        let classic_dark_text = egui::Color32::BLACK;
        let classic_white = egui::Color32::WHITE;
        let classic_blue_accent = egui::Color32::from_rgb(0, 100, 180); 

        style.visuals.window_fill = classic_gray_background;
        style.visuals.panel_fill = classic_gray_background;
        style.visuals.override_text_color = Some(classic_dark_text);

        style.visuals.widgets.noninteractive.rounding = egui::Rounding::ZERO; 
        style.visuals.widgets.inactive.rounding = egui::Rounding::ZERO;
        style.visuals.widgets.hovered.rounding = egui::Rounding::ZERO;
        style.visuals.widgets.active.rounding = egui::Rounding::ZERO;
        style.visuals.widgets.open.rounding = egui::Rounding::ZERO;
        
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY); 
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, classic_dark_text); 
        style.visuals.widgets.inactive.bg_fill = classic_gray_background; 

        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui::Color32::GRAY);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, classic_dark_text);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(230, 230, 230);

        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY); 
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, classic_dark_text);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(200, 200, 200);

        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(200, 200, 200); 
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(150, 150, 150);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(120, 120, 120); 
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(100, 100, 100); 
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY);
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::DARK_GRAY);

        style.visuals.extreme_bg_color = classic_white; 
        style.visuals.selection.bg_fill = classic_blue_accent; 
        style.visuals.selection.stroke = egui::Stroke::new(1.0, classic_white); 
        style.visuals.hyperlink_color = classic_blue_accent;
        style.visuals.widgets.inactive.bg_fill = classic_gray_background; 

        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::new(14.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::new(14.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(16.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::new(13.0, egui::FontFamily::Monospace));
        style.text_styles.insert(egui::TextStyle::Small, egui::FontId::new(12.0, egui::FontFamily::Proportional));

        _cc.egui_ctx.set_style(style);

        let app_data_internal = NostrStatusAppInternal {
            is_logged_in: false,
            status_message_input: String::new(),
            secret_key_input: String::new(),
            passphrase_input: String::new(),
            confirm_passphrase_input: String::new(),
            nostr_client: None,
            my_keys: None,
            followed_pubkeys: HashSet::new(),
            followed_pubkeys_display: String::new(),
            status_timeline_display: String::new(),
            should_repaint: false,
            is_loading: false,
            current_tab: AppTab::Home,
            connected_relays_display: String::new(),
        };
        let data = Arc::new(Mutex::new(app_data_internal));

        // アプリケーション起動時に設定ファイルをチェック
        let data_clone = data.clone();
        let runtime_handle = runtime.handle().clone();

        runtime_handle.spawn(async move {
            let mut app_data = data_clone.lock().unwrap();
            println!("Checking config file...");

            if Path::new(CONFIG_FILE).exists() {
                println!("Existing user: Please enter your passphrase.");
            } else {
                println!("First-time setup: Enter your secret key and set a passphrase.");
            }
            app_data.should_repaint = true;
        });
        
        Self { data, runtime }
    }
}

// NIP-65とフォールバックを考慮したリレー接続関数
async fn connect_to_relays_with_nip65(client: &Client, keys: &Keys) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap_relays = vec![
        "wss://purplepag.es",    
        "wss://directory.yabu.me", 
    ];

    let client_opts = Options::new().connection_timeout(Some(Duration::from_secs(30)));
    let discover_client = Client::with_opts(&*keys, client_opts.clone()); // A dedicated client for discovery

    let mut status_log = String::new();
    status_log.push_str("NIP-65リレーリストを取得するためにDiscoverリレーに接続中...\n");
    for relay_url in &bootstrap_relays {
        if let Err(e) = discover_client.add_relay(*relay_url).await { // Add to discover_client
            status_log.push_str(&format!("  Discoverリレー追加失敗: {} - エラー: {}\n", *relay_url, e));
        } else {
            status_log.push_str(&format!("  Discoverリレー追加: {}\n", *relay_url));
        }
    }
    discover_client.connect().await; // Connect discover_client
    tokio::time::sleep(Duration::from_secs(2)).await; // Discoverリレー接続安定待ち

    let filter = Filter::new()
        .authors(vec![keys.public_key()])
        .kind(Kind::RelayList);

    status_log.push_str("NIP-65リレーリストイベントを検索中 (最大10秒)..\n"); // Timeout reduced
    let timeout_filter_id = discover_client.subscribe(vec![filter], Some(SubscribeAutoCloseOptions::default())).await;

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
                        for tag in &event.tags {
                            if let Tag::RelayMetadata(url, policy) = tag {
                                let url_string = url.to_string();
                                let policy_string = match policy {
                                    Some(nostr::RelayMetadata::Write) => Some("write".to_string()),
                                    Some(nostr::RelayMetadata::Read) => Some("read".to_string()),
                                    None => None,
                                };
                                nip65_relays.push((url_string, policy_string));
                            }
                        }
                        received_nip65_event = true;
                        break;
                    }
                }
            }
        } => {}
    }

    discover_client.unsubscribe(timeout_filter_id).await;
    discover_client.shutdown().await?;

    status_log.push_str("--- NIP-65で受信したリレー情報 ---\n");
    if nip65_relays.is_empty() {
        status_log.push_str("  有効なNIP-65リレーは受信しませんでした。\n");
    } else {
        for (url, policy) in &nip65_relays {
            status_log.push_str(&format!("  URL: {}, Policy: {:?}\n", url, policy));
        }
    }
    status_log.push_str("---------------------------------\n");

    let connected_relays_count: usize;
    let mut current_connected_relays = Vec::new();

    if received_nip65_event && !nip65_relays.is_empty() {
        status_log.push_str("\nNIP-65で検出されたリレーに接続中...\n");
        let _ = client.remove_all_relays().await;

        for (url, policy) in nip65_relays {
            if policy.as_deref() == Some("write") || policy.is_none() {
                if let Err(e) = client.add_relay(url.as_str()).await {
                    status_log.push_str(&format!("  リレー追加失敗: {} - エラー: {}\n", url, e));
                } else {
                    status_log.push_str(&format!("  リレー追加: {}\n", url));
                    current_connected_relays.push(url);
                }
            }
        }
        client.connect().await;
        connected_relays_count = client.relays().await.len();
        status_log.push_str(&format!("{}つのリレーに接続しました。\n", connected_relays_count));
    } else {
        status_log.push_str("\nNIP-65リレーリストが見つからなかったため、デフォルトのリレーに接続します。\n");
        let _ = client.remove_all_relays().await;
        
        let fallback_relays = ["wss://relay.damus.io", "wss://relay.nostr.wirednet.jp", "wss://yabu.me"];
        for relay_url in fallback_relays.iter() {
            if let Err(e) = client.add_relay(*relay_url).await {
                status_log.push_str(&format!("  デフォルトリレー追加失敗: {} - エラー: {}\n", *relay_url, e));
            } else {
                status_log.push_str(&format!("  デフォルトリレー追加: {}\n", *relay_url));
                current_connected_relays.push(relay_url.to_string());
            }
        }
        client.connect().await;
        connected_relays_count = client.relays().await.len();
        status_log.push_str(&format!("デフォルトのリレーに接続しました。{}つのリレー。\n", connected_relays_count));
    }

    if connected_relays_count == 0 {
        return Err("接続できるリレーがありません。".into());
    }

    // 接続が安定するまで少し待つ
    tokio::time::sleep(Duration::from_secs(2)).await;
    status_log.push_str("リレー接続が安定しました。\n");

    Ok(format!("{}\n\n--- 現在接続中のリレー ---\n{}", status_log, current_connected_relays.join("\n")))
}

impl eframe::App for NostrStatusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // MutexGuardをupdate関数全体のスコープで保持
        let mut app_data = self.data.lock().unwrap(); 

        // app_data_arc をクローンして非同期タスクに渡す
        let app_data_arc_clone = self.data.clone();
        let runtime_handle = self.runtime.handle().clone();

        egui::SidePanel::left("side_panel")
            .min_width(150.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Nostr Status App");
                ui.separator();
                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.selectable_value(&mut app_data.current_tab, AppTab::Home, "🏠 Home");
                    if app_data.is_logged_in {
                        ui.selectable_value(&mut app_data.current_tab, AppTab::Relays, "📡 Relays");
                        ui.selectable_value(&mut app_data.current_tab, AppTab::Profile, "👤 Profile");
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(
                match app_data.current_tab {
                    AppTab::Home => "Home (Status & Timeline)",
                    AppTab::Relays => "Relay Management",
                    AppTab::Profile => "User Profile",
                }
            );
            ui.separator();
            ui.add_space(10.0);

            ui.add_enabled_ui(!app_data.is_loading, |ui| { 
                if !app_data.is_logged_in {
                    if app_data.current_tab == AppTab::Home {
                        ui.group(|ui| {
                            ui.heading("Login or Register");
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label("Secret Key (nsec or hex, for first-time setup):");
                                ui.add(egui::TextEdit::singleline(&mut app_data.secret_key_input)
                                    .hint_text("Enter your nsec or hex secret key here"));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Passphrase:");
                                ui.add(egui::TextEdit::singleline(&mut app_data.passphrase_input)
                                    .password(true)
                                    .hint_text("Your secure passphrase"));
                            });

                            if Path::new(CONFIG_FILE).exists() {
                                if ui.button(egui::RichText::new("🔑 Login with Passphrase").strong()).clicked() && !app_data.is_loading {
                                    let passphrase = app_data.passphrase_input.clone();
                                    
                                    // ロード状態と再描画フラグを更新（現在のMutexGuardで）
                                    app_data.is_loading = true;
                                    app_data.should_repaint = true;
                                    println!("Attempting to login...");
                                    
                                    // app_data_arc_clone を async move ブロックに渡す
                                    let cloned_app_data_arc = app_data_arc_clone.clone(); 
                                    runtime_handle.spawn(async move {
                                        let login_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                                            // --- 1. 鍵の復号 ---
                                            println!("Attempting to decrypt secret key...");
                                            let keys = (|| -> Result<Keys, Box<dyn std::error::Error + Send + Sync>> {
                                                let config_str = fs::read_to_string(CONFIG_FILE)?;
                                                let config: Config = serde_json::from_str(&config_str)?;
                                                let retrieved_salt_bytes = general_purpose::STANDARD.decode(&config.salt)?;
                                                let mut derived_key_bytes = [0u8; 32];
                                                pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &retrieved_salt_bytes, 100_000, &mut derived_key_bytes);
                                                let cipher_key = Key::from_slice(&derived_key_bytes);
                                                let cipher = ChaCha20Poly1305::new(cipher_key);
                                                let nip49_encoded = config.encrypted_secret_key;
                                                if !nip49_encoded.starts_with("#nip49:") { return Err("設定ファイルのNIP-49フォーマットが無効です。".into()); }
                                                let decoded_bytes = general_purpose::STANDARD.decode(&nip49_encoded[7..])?;
                                                if decoded_bytes.len() < 12 { return Err("設定ファイルのNIP-49ペイロードが短すぎます。".into()); }
                                                let (ciphertext_and_tag, retrieved_nonce_bytes) = decoded_bytes.split_at(decoded_bytes.len() - 12);
                                                let retrieved_nonce = Nonce::from_slice(retrieved_nonce_bytes);
                                                let decrypted_bytes = cipher.decrypt(retrieved_nonce, ciphertext_and_tag).map_err(|_| "パスフレーズが正しくありません。")?;
                                                let decrypted_secret_key_hex = hex::encode(&decrypted_bytes);
                                                Ok(Keys::parse(&decrypted_secret_key_hex)?)
                                            })()?;
                                            println!("Secret key decrypted successfully. Public Key: {}", keys.public_key().to_bech32().unwrap_or_default());
                                            
                                            let client = Client::new(&keys);

                                            // --- 2. リレー接続 (NIP-65) ---
                                            println!("Connecting to relays...");
                                            let log_message = connect_to_relays_with_nip65(&client, &keys).await?;
                                            println!("Relay connection process finished.\n{}", log_message);
                                            
                                            // --- 3. フォローリスト取得 (NIP-02) ---
                                            println!("Fetching NIP-02 contact list...");
                                            let nip02_filter = Filter::new().authors(vec![keys.public_key()]).kind(Kind::ContactList).limit(1);
                                            let nip02_filter_id = client.subscribe(vec![nip02_filter], Some(SubscribeAutoCloseOptions::default())).await;
                                            
                                            let mut followed_pubkeys = HashSet::new();
                                            let mut received_nip02 = false;

                                            tokio::select! {
                                                _ = tokio::time::sleep(Duration::from_secs(10)) => {} // Timeout reduced
                                                _ = async {
                                                    let mut notifications = client.notifications();
                                                    while let Ok(notification) = notifications.recv().await {
                                                        if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                                                            if event.kind == Kind::ContactList && event.pubkey == keys.public_key() {
                                                                println!("Contact list event received.");
                                                                for tag in &event.tags { if let Tag::PublicKey { public_key, .. } = tag { followed_pubkeys.insert(*public_key); } }
                                                                received_nip02 = true;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                } => {},
                                            }
                                            client.unsubscribe(nip02_filter_id).await;

                                            if !received_nip02 {
                                                return Err("Failed to fetch contact list (timed out or not found).".into());
                                            }
                                            println!("Fetched {} followed pubkeys.", followed_pubkeys.len());

                                            // --- 4. タイムライン取得 (NIP-38) ---
                                            let mut final_timeline_display = "No timeline available.".to_string();
                                            if !followed_pubkeys.is_empty() {
                                                println!("Fetching NIP-38 status timeline...");
                                                let timeline_filter = Filter::new().authors(followed_pubkeys.iter().cloned()).kind(Kind::ParameterizedReplaceable(30315)).limit(20);
                                                let timeline_filter_id = client.subscribe(vec![timeline_filter], Some(SubscribeAutoCloseOptions::default())).await;
                                                let mut collected_statuses = Vec::new();
                                                tokio::select! {
                                                    _ = tokio::time::sleep(Duration::from_secs(10)) => { println!("Status timeline search timed out."); } // Timeout reduced
                                                    _ = async {
                                                        let mut notifications = client.notifications();
                                                        while let Ok(notification) = notifications.recv().await {
                                                            if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                                                                if event.kind == Kind::ParameterizedReplaceable(30315) {
                                                                    let d_tag = event.tags.iter().find_map(|t| if let Tag::Identifier(d) = t { Some(d.clone()) } else { None }).unwrap_or_else(|| "general".to_string());
                                                                    collected_statuses.push((event.pubkey, d_tag, event.content.clone()));
                                                                }
                                                            }
                                                        }
                                                    } => {},
                                                }
                                                client.unsubscribe(timeline_filter_id).await;
                                                
                                                if !collected_statuses.is_empty() {
                                                    final_timeline_display = collected_statuses.iter().map(|(pk, d, c)| format!("{} ({}) says: {}", pk.to_bech32().unwrap_or_default(), d, c)).collect::<Vec<_>>().join("\n\n");
                                                    println!("Fetched {} statuses.", collected_statuses.len());
                                                } else {
                                                    final_timeline_display = "No NIP-38 statuses found for followed users.".to_string();
                                                    println!("No statuses found.");
                                                }
                                            }
                                            
                                            // --- 5. 最終的なUI状態の更新 ---
                                            let mut app_data = cloned_app_data_arc.lock().unwrap();
                                            app_data.my_keys = Some(keys);
                                            app_data.nostr_client = Some(client);
                                            app_data.followed_pubkeys = followed_pubkeys.clone();
                                            app_data.followed_pubkeys_display = followed_pubkeys.iter().map(|pk| pk.to_bech32().unwrap_or_default()).collect::<Vec<_>>().join("\n");
                                            app_data.status_timeline_display = final_timeline_display;
                                            if let Some(pos) = log_message.find("--- 現在接続中のリレー ---") {
                                                app_data.connected_relays_display = log_message[pos..].to_string();
                                            }
                                            app_data.is_logged_in = true;
                                            app_data.current_tab = AppTab::Home;
                                            println!("Login process complete!");

                                            Ok(())
                                        }.await;

                                        if let Err(e) = login_result {
                                            eprintln!("Login failed: {}", e);
                                            // 失敗した場合、Clientをシャットダウン
                                            // clientをOptionから取り出して所有権を得る
                                            let client_to_shutdown = {
                                                let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                                                app_data_in_task.nostr_client.take() // Option::take()で所有権を取得
                                            };
                                            if let Some(client) = client_to_shutdown {
                                                if let Err(e) = client.shutdown().await {
                                                     eprintln!("Failed to shutdown client: {}", e);
                                                }
                                            }
                                        }

                                        let mut app_data_in_task = cloned_app_data_arc.lock().unwrap();
                                        app_data_in_task.is_loading = false;
                                        app_data_in_task.should_repaint = true; // 再描画をリクエスト
                                    });
                                }
                            } else {
                                ui.horizontal(|ui| {
                                    ui.label("Confirm Passphrase:");
                                    ui.add(egui::TextEdit::singleline(&mut app_data.confirm_passphrase_input)
                                        .password(true)
                                    .hint_text("Confirm your passphrase"));
                                });

                                if ui.button(egui::RichText::new("✨ Register New Key").strong()).clicked() && !app_data.is_loading {
                                    let secret_key_input = app_data.secret_key_input.clone();
                                    let passphrase = app_data.passphrase_input.clone();
                                    let confirm_passphrase = app_data.confirm_passphrase_input.clone();
                                    
                                    app_data.is_loading = true;
                                    app_data.should_repaint = true;
                                    println!("Registering new key...");

                                    let cloned_app_data_arc = app_data_arc_clone.clone();
                                    runtime_handle.spawn(async move {
                                        if passphrase != confirm_passphrase {
                                            eprintln!("Error: Passphrases do not match!");
                                            let mut current_app_data = cloned_app_data_arc.lock().unwrap();
                                            current_app_data.is_loading = false;
                                            current_app_data.should_repaint = true; // 再描画をリクエスト
                                            return;
                                        }

                                        let result: Result<Keys, Box<dyn std::error::Error + Send + Sync>> = (|| {
                                            let user_provided_keys = Keys::parse(&secret_key_input)?;
                                            if user_provided_keys.secret_key().is_err() { return Err("入力された秘密鍵は無効です。".into()); }
                                            let mut salt_bytes = [0u8; 16];
                                            OsRng.fill(&mut salt_bytes);
                                            let salt_base64 = general_purpose::STANDARD.encode(&salt_bytes);
                                            let mut derived_key_bytes = [0u8; 32];
                                            pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt_bytes, 100_000, &mut derived_key_bytes);
                                            let cipher_key = Key::from_slice(&derived_key_bytes);
                                            let cipher = ChaCha20Poly1305::new(cipher_key);
                                            let plaintext_bytes = user_provided_keys.secret_key()?.to_secret_bytes();
                                            let mut nonce_bytes: [u8; 12] = [0u8; 12];
                                            OsRng.fill(&mut nonce_bytes);
                                            let nonce = Nonce::from_slice(&nonce_bytes);
                                            let ciphertext_with_tag = cipher.encrypt(nonce, plaintext_bytes.as_slice()).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { format!("NIP-49 暗号化エラー: {:?}", e).into() })?;
                                            let mut encoded_data = ciphertext_with_tag.clone();
                                            encoded_data.extend_from_slice(nonce_bytes.as_ref());
                                            let nip49_encoded = format!("#nip49:{}", general_purpose::STANDARD.encode(&encoded_data));
                                            let config = Config { encrypted_secret_key: nip49_encoded, salt: salt_base64 };
                                            let config_json = serde_json::to_string_pretty(&config)?;
                                            fs::write(CONFIG_FILE, config_json)?;
                                            Ok(user_provided_keys)
                                        })();

                                        let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                        app_data_async.is_loading = false;
                                        if let Ok(keys) = result {
                                            app_data_async.my_keys = Some(keys.clone());
                                            let client = Client::new(&keys);
                                            app_data_async.nostr_client = Some(client);
                                            app_data_async.is_logged_in = true;
                                            println!("Registered and logged in. Public Key: {}", keys.public_key().to_bech32().unwrap_or_default());
                                            app_data_async.current_tab = AppTab::Home;
                                        } else {
                                            eprintln!("Failed to register new key: {}", result.unwrap_err());
                                        }
                                        app_data_async.should_repaint = true; // 再描画をリクエスト
                                    });
                                }
                            }
                        }); 
                    }
                } else {
                    match app_data.current_tab {
                        AppTab::Home => {
                            ui.group(|ui| {
                                ui.heading("Set Status (NIP-38)");
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(format!("Characters: {}/{}", app_data.status_message_input.chars().count(), MAX_STATUS_LENGTH));
                                    if app_data.status_message_input.chars().count() > MAX_STATUS_LENGTH {
                                        ui.label(egui::RichText::new("Too Long!").color(egui::Color32::RED).strong());
                                    }
                                });
                                ui.add(egui::TextEdit::multiline(&mut app_data.status_message_input)
                                    .desired_rows(3)
                                    .hint_text("What's on your mind? (max 140 chars)"));
                                ui.add_space(10.0);

                                if ui.button(egui::RichText::new("🚀 Publish Status").strong()).clicked() && !app_data.is_loading {
                                    let status_message = app_data.status_message_input.clone();
                                    let client_clone_nip38_send = app_data.nostr_client.as_ref().unwrap().clone(); 
                                    let keys_clone_nip38_send = app_data.my_keys.clone().unwrap();
                                    
                                    app_data.is_loading = true;
                                    app_data.should_repaint = true;
                                    println!("Publishing NIP-38 status...");

                                    if status_message.chars().count() > MAX_STATUS_LENGTH {
                                        eprintln!("Error: Status too long! Max {} characters.", MAX_STATUS_LENGTH);
                                        // `app_data`はまだスコープ内なので、直接更新
                                        app_data.is_loading = false;
                                        app_data.should_repaint = true; // 再描画をリクエスト
                                        return;
                                    }
                                    
                                    let cloned_app_data_arc = app_data_arc_clone.clone(); // async moveに渡す
                                    runtime_handle.spawn(async move {
                                        let d_tag_value = "general".to_string();
                                        let event = EventBuilder::new(Kind::ParameterizedReplaceable(30315), status_message.clone(), vec![Tag::Identifier(d_tag_value)]).to_event(&keys_clone_nip38_send);
                                        match event {
                                            Ok(event) => match client_clone_nip38_send.send_event(event).await {
                                                Ok(event_id) => {
                                                    println!("Status published! Event ID: {}", event_id);
                                                    cloned_app_data_arc.lock().unwrap().status_message_input.clear();
                                                }
                                                Err(e) => eprintln!("Failed to publish status: {}", e),
                                            },
                                            Err(e) => eprintln!("Failed to create event: {}", e),
                                        }
                                        let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                        app_data_async.is_loading = false;
                                        app_data_async.should_repaint = true; // 再描画をリクエスト
                                    });
                                }
                            });

                            ui.add_space(20.0);
                            ui.group(|ui| {
                                ui.heading("Status Timeline");
                                ui.add_space(10.0);
                                if ui.button(egui::RichText::new("🔄 Fetch Latest Statuses").strong()).clicked() && !app_data.is_loading {
                                    let client_clone_nip38_fetch = app_data.nostr_client.as_ref().unwrap().clone(); 
                                    let followed_pubkeys_clone_nip38_fetch = app_data.followed_pubkeys.clone();
                                    
                                    app_data.is_loading = true;
                                    app_data.should_repaint = true;
                                    println!("Fetching NIP-38 status timeline...");

                                    let cloned_app_data_arc = app_data_arc_clone.clone(); // async moveに渡す
                                    runtime_handle.spawn(async move {
                                        if followed_pubkeys_clone_nip38_fetch.is_empty() {
                                            println!("No followed users to fetch status from. Please fetch NIP-02 contacts first.");
                                            let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                            app_data_async.status_timeline_display = "No timeline available without followed users.".to_string();
                                            app_data_async.is_loading = false;
                                            app_data_async.should_repaint = true; // 再描画をリクエスト
                                            return;
                                        }

                                        let timeline_filter = Filter::new().authors(followed_pubkeys_clone_nip38_fetch.into_iter()).kind(Kind::ParameterizedReplaceable(30315)).limit(20);
                                        let timeline_filter_id = client_clone_nip38_fetch.subscribe(vec![timeline_filter], Some(SubscribeAutoCloseOptions::default())).await;
                                        let mut collected_statuses = Vec::new();
                                        tokio::select! {
                                            _ = tokio::time::sleep(Duration::from_secs(10)) => { println!("Status timeline search timed out."); } // Timeout reduced
                                            _ = async {
                                                let mut notifications = client_clone_nip38_fetch.notifications();
                                                while let Ok(notification) = notifications.recv().await {
                                                    if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                                                        if event.kind == Kind::ParameterizedReplaceable(30315) {
                                                            let d_tag = event.tags.iter().find_map(|t| if let Tag::Identifier(d) = t { Some(d.clone()) } else { None }).unwrap_or_else(|| "general".to_string());
                                                            collected_statuses.push((event.pubkey, d_tag, event.content.clone()));
                                                        }
                                                    }
                                                }
                                            } => {},
                                        }
                                        client_clone_nip38_fetch.unsubscribe(timeline_filter_id).await;

                                        let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                        app_data_async.is_loading = false;
                                        if !collected_statuses.is_empty() {
                                            app_data_async.status_timeline_display = collected_statuses.iter().map(|(pk, d, c)| format!("{} ({}) says: {}", pk.to_bech32().unwrap_or_default(), d, c)).collect::<Vec<_>>().join("\n\n");
                                            println!("Fetched {} statuses.", collected_statuses.len());
                                        } else {
                                            app_data_async.status_timeline_display = "No NIP-38 statuses found for followed users.".to_string();
                                            println!("No statuses found.");
                                        }
                                        app_data_async.should_repaint = true; // 再描画をリクエスト
                                    });
                                }
                                ui.add_space(10.0);
                                egui::ScrollArea::vertical().id_source("timeline_scroll_area").max_height(250.0).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut app_data.status_timeline_display)
                                        .desired_width(ui.available_width())
                                        .interactive(false));
                                });
                            });
                        },
                        AppTab::Relays => {
                            ui.group(|ui| {
                                ui.heading("Relay Connection");
                                ui.add_space(10.0);
                                if ui.button(egui::RichText::new("🔗 Re-Connect to Relays (NIP-65)").strong()).clicked() && !app_data.is_loading {
                                    let client_clone = app_data.nostr_client.as_ref().unwrap().clone(); 
                                    let keys_clone = app_data.my_keys.clone().unwrap();
                                    
                                    app_data.is_loading = true;
                                    app_data.should_repaint = true;
                                    println!("Re-connecting to relays...");
                                    
                                    let cloned_app_data_arc = app_data_arc_clone.clone(); // async moveに渡す
                                    runtime_handle.spawn(async move {
                                        match connect_to_relays_with_nip65(&client_clone, &keys_clone).await {
                                            Ok(log_message) => {
                                                println!("Relay connection successful!\n{}", log_message);
                                                let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                                if let Some(pos) = log_message.find("--- 現在接続中のリレー ---") {
                                                    app_data_async.connected_relays_display = log_message[pos..].to_string();
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to connect to relays: {}", e);
                                            }
                                        }
                                        let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                                        app_data_async.is_loading = false;
                                        app_data_async.should_repaint = true; // 再描画をリクエスト
                                    });
                                }
                                ui.add_space(10.0);
                                egui::ScrollArea::vertical().id_source("relay_connection_scroll_area").max_height(250.0).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut app_data.connected_relays_display)
                                        .desired_width(ui.available_width())
                                        .interactive(false));
                                }); // 👈 ここが変更されました
                            });
                        },
                        AppTab::Profile => {
                            ui.group(|ui| {
                                ui.heading("Your Profile");
                                ui.add_space(10.0);
                                
                                ui.heading("My Public Key");
                                ui.add_space(5.0);
                                let public_key_bech32 = app_data.my_keys.as_ref().map_or("N/A".to_string(), |k| k.public_key().to_bech32().unwrap_or_default());
                                ui.horizontal(|ui| {
                                    ui.label(public_key_bech32.clone());
                                    if ui.button("📋 Copy").clicked() {
                                        ctx.copy_text(public_key_bech32);
                                        println!("Public key copied to clipboard!");
                                        app_data.should_repaint = true; // 再描画をリクエスト
                                    }
                                });
                                ui.add_space(20.0);
                                ui.label(egui::RichText::new("Future Feature: Edit your profile metadata (NIP-01) here.").strong().color(egui::Color32::from_rgb(0, 0, 150)));
                                
                                // --- ログアウトボタン ---
                                ui.add_space(50.0); 
                                ui.separator();
                                if ui.button(egui::RichText::new("↩️ Logout").color(egui::Color32::RED)).clicked() {
                                    // MutexGuardを解放する前に、所有権をタスクに移動させる
                                    let client_to_shutdown = app_data.nostr_client.take(); // Option::take()で所有権を取得
                                    
                                    // UIの状態をリセット
                                    app_data.is_logged_in = false;
                                    app_data.my_keys = None;
                                    app_data.followed_pubkeys.clear();
                                    app_data.followed_pubkeys_display.clear();
                                    app_data.status_timeline_display.clear();
                                    app_data.status_message_input.clear();
                                    app_data.passphrase_input.clear();
                                    app_data.confirm_passphrase_input.clear();
                                    app_data.secret_key_input.clear();
                                    app_data.current_tab = AppTab::Home;
                                    app_data.should_repaint = true; // 再描画をリクエスト
                                    println!("Logged out.");

                                    // Clientのシャットダウンを非同期タスクで行う
                                    if let Some(client) = client_to_shutdown {
                                        runtime_handle.spawn(async move {
                                            if let Err(e) = client.shutdown().await {
                                                eprintln!("Failed to shutdown client on logout: {}", e);
                                            }
                                        });
                                    }
                                }
                            });
                        },
                    }
                }
            }); 
        });

        // update メソッドの最後に should_repaint をチェックし、再描画をリクエスト
        if app_data.should_repaint {
            ctx.request_repaint();
            app_data.should_repaint = false; // リクエスト後にフラグをリセット
        }

        // ロード中もUIを常に更新するようリクエスト
        if app_data.is_loading {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    // env_logger::init(); // 必要に応じて有効化

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Nostr NIP-38 Status Sender",
        options,
        Box::new(|cc| Ok(Box::new(NostrStatusApp::new(cc)))),
    )
}
