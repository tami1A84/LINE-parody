use eframe::egui;
use nostr::nips::nip47::{NostrWalletConnectURI, Response};
use nostr::{Event, Filter, JsonUtil, Kind, Keys, SingleLetterTag, TagKind};
use nostr_sdk::Client;
use serde_json;
use std::fs;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;

use crate::nostr_client::get_profile_metadata;
use crate::types::{Config, NostrStatusAppInternal, ProfileMetadata, ZapReceipt};
use crate::{nip49, CONFIG_FILE};
use chrono::{DateTime, Utc};
use lightning_invoice::Bolt11Invoice;

pub fn draw_wallet_view(
    ui: &mut egui::Ui,
    app_data: &mut NostrStatusAppInternal,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
    runtime_handle: Handle,
) {
    ui.heading("ウォレット");
    ui.add_space(10.0);

    if !app_data.is_logged_in {
        ui.label("ウォレット機能を使うにはログインしてください。");
        return;
    }

    if app_data.nwc.is_some() {
        draw_wallet_details(ui, app_data, app_data_arc.clone(), runtime_handle);
    } else {
        draw_setup_view(ui, app_data, app_data_arc.clone(), runtime_handle);
    }

    if let Some(error) = &app_data.nwc_error {
        ui.add_space(10.0);
        ui.colored_label(egui::Color32::RED, error);
    }
}

fn draw_wallet_details(
    ui: &mut egui::Ui,
    app_data: &mut NostrStatusAppInternal,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
    runtime_handle: Handle,
) {
    ui.label("ウォレット接続済み");
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        let refresh_button = ui.add_enabled(!app_data.is_fetching_zap_history, egui::Button::new("履歴を更新"));
        if refresh_button.clicked() {
            let app_data_clone = app_data_arc.clone();
            runtime_handle.spawn(async move {
                if let Err(e) = get_zap_history(app_data_clone.clone()).await {
                    let mut app_data = app_data_clone.lock().unwrap();
                    app_data.nwc_error = Some(format!("Zap履歴の取得エラー: {}", e));
                }
            });
        }

        if app_data.is_fetching_zap_history {
            ui.add_space(10.0);
            ui.spinner();
            ui.label(&app_data.zap_history_fetch_status);
        }
    });

    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        if app_data.zap_history.is_empty() {
            ui.label("Zap履歴はありません。");
        } else {
            for zap in &app_data.zap_history {
                ui.horizontal(|ui| {
                    let name = if zap.recipient_metadata.name.is_empty() {
                        "不明なユーザー"
                    } else {
                        &zap.recipient_metadata.name
                    };
                    ui.label(name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{} sats", zap.amount_msats / 1000));
                        let date = DateTime::<Utc>::from_timestamp(zap.created_at.as_u64() as i64, 0)
                            .unwrap()
                            .format("%Y-%m-%d %H:%M")
                            .to_string();
                        ui.label(date);
                    });
                });
                ui.separator();
            }
        }
    });
}

fn draw_setup_view(
    ui: &mut egui::Ui,
    app_data: &mut NostrStatusAppInternal,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
    runtime_handle: Handle,
) {
    ui.label("Nostrウォレットに接続");
    ui.add_space(5.0);
    ui.label("Nostr Wallet ConnectのURIと、暗号化のためのメインパスフレーズを入力してください。");

    ui.horizontal(|ui| {
        ui.label("NWC URI:");
        ui.text_edit_singleline(&mut app_data.nwc_uri_input);
    });

    ui.horizontal(|ui| {
        ui.label("アプリのパスフレーズ:");
        ui.add(egui::TextEdit::singleline(&mut app_data.nwc_passphrase_input).password(true));
    });

    if ui.button("保存して接続").clicked() {
        let nwc_uri = app_data.nwc_uri_input.clone();
        let passphrase = app_data.nwc_passphrase_input.clone();
        app_data.nwc_passphrase_input.clear(); // Clear passphrase after use
        let app_data_clone = app_data_arc.clone();

        runtime_handle.spawn(async move {
            match save_and_connect(nwc_uri, passphrase, app_data_clone.clone()).await {
                Ok(_) => {
                    let mut app_data = app_data_clone.lock().unwrap();
                    app_data.nwc_error = None;
                }
                Err(e) => {
                    let mut app_data = app_data_clone.lock().unwrap();
                    app_data.nwc_error = Some(format!("保存と接続に失敗しました: {}", e));
                }
            }
        });
    }
}

async fn save_and_connect(
    nwc_uri_str: String,
    passphrase: String,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if passphrase.is_empty() {
        return Err("パスフレーズは空にできません".into());
    }
    let nwc_uri = NostrWalletConnectURI::from_str(&nwc_uri_str)?;

    // Read existing config to get the salt
    let config_str = fs::read_to_string(CONFIG_FILE)?;
    let mut config: Config = serde_json::from_str(&config_str)?;

    // Verify passphrase by trying to decrypt the main secret key
    let _ = nip49::decrypt(&config.encrypted_secret_key, &passphrase, &config.salt)?;

    // Encrypt NWC URI with the same salt and passphrase
    let encrypted_nwc_uri =
        nip49::encrypt_with_salt(nwc_uri_str.as_bytes(), &passphrase, &config.salt)?;
    config.encrypted_nwc_uri = Some(encrypted_nwc_uri);

    // Save updated config
    let config_json = serde_json::to_string_pretty(&config)?;
    fs::write(CONFIG_FILE, config_json)?;

    connect_nwc(nwc_uri, app_data_arc).await?;

    Ok(())
}

pub async fn connect_nwc(
    nwc_uri: NostrWalletConnectURI,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let keys = Keys::new(nwc_uri.secret.clone());
    let client = Client::new(keys);

    if let Some(relay_url) = nwc_uri.relays.first() {
        client.add_relay(relay_url.to_string()).await?;
    } else {
        return Err("NWC URIにリレーURLが含まれていません".into());
    }
    client.connect().await;

    // Spawn listener task
    let client_clone = client.clone();
    let nwc_uri_clone = nwc_uri.clone();
    let app_data_clone = app_data_arc.clone();
    tokio::spawn(async move {
        listen_for_nwc_responses(client_clone, nwc_uri_clone, app_data_clone).await;
    });

    {
        let mut app_data = app_data_arc.lock().unwrap();
        app_data.nwc_client = Some(client);
        app_data.nwc = Some(nwc_uri.clone());
    }

    // When connecting, automatically fetch zap history
    let app_data_clone = app_data_arc.clone();
    tokio::spawn(async move {
        if let Err(e) = get_zap_history(app_data_clone.clone()).await {
            let mut app_data = app_data_clone.lock().unwrap();
            app_data.nwc_error = Some(format!("Zap履歴の取得エラー: {}", e));
        }
    });

    Ok(())
}

async fn listen_for_nwc_responses(
    _client: Client,
    _nwc: NostrWalletConnectURI,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
) {
    // We keep the listener active for potential future uses,
    // like real-time updates, but for now it only handles PayInvoice responses.
    let mut notifications = _client.notifications();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                // Keep the loop from being too tight
            }
            Ok(notification) = notifications.recv() => {
                if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::WalletConnectResponse {
                         if let Ok(decrypted_response) = Response::from_event(&_nwc, &event) {
                            let mut app_data = app_data_arc.lock().unwrap();
                            if let Some(res) = decrypted_response.result {
                                match res {
                                    nostr::nips::nip47::ResponseResult::PayInvoice(_pay_invoice_res) => {
                                        println!("ZAP成功！");
                                        // Here you might want to trigger a refresh of the zap history
                                    },
                                    _ => {
                                        // Other responses are ignored for now
                                    }
                                }
                            } else if let Some(error) = decrypted_response.error {
                                app_data.nwc_error = Some(format!("NWCエラー: {}", error.message));
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn get_zap_history(
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (client, my_pubkey) = {
        let mut app_data = app_data_arc.lock().unwrap();
        app_data.is_fetching_zap_history = true;
        app_data.zap_history_fetch_status = "取得中...".to_string();
        let client = app_data
            .nostr_client
            .as_ref()
            .cloned()
            .ok_or("Nostrクライアントが接続されていません")?;
        let my_pubkey = app_data
            .my_keys
            .as_ref()
            .map(|k| k.public_key())
            .ok_or("ログインしていません")?;
        (client, my_pubkey)
    };

    let filter = Filter::new()
        .kind(Kind::ZapReceipt)
        .custom_tag(SingleLetterTag::from_char('P').unwrap(), my_pubkey.to_string())
        .limit(100); // Get last 100 zaps

    let relays = client.relays().await;
    let relay_urls: Vec<String> = relays.keys().map(|url| url.to_string()).collect();
    let events = client.fetch_events_from(relay_urls, filter, std::time::Duration::from_secs(10)).await;

    // The rest of the function will run regardless of whether events were found or not,
    // so we can set is_fetching to false after this point.
    // We will handle the result of event fetching below.
    {
        let mut app_data = app_data_arc.lock().unwrap();
        app_data.is_fetching_zap_history = false;
        app_data.zap_history_fetch_status = String::new(); // Clear status text
    }


    match events {
        Ok(events) => {
            let mut zap_receipts = Vec::new();

            for event in events {
                if let Ok(receipt) = parse_zap_receipt(event, &client).await {
                    zap_receipts.push(receipt);
                }
            }

            // Sort by creation date, newest first
            zap_receipts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            {
                let mut app_data = app_data_arc.lock().unwrap();
                app_data.zap_history = zap_receipts;
                app_data.nwc_error = None;
            }
        },
        Err(e) => {
             let mut app_data = app_data_arc.lock().unwrap();
             app_data.nwc_error = Some(format!("Zap履歴の取得エラー: {}", e));
        }
    }


    Ok(())
}

async fn parse_zap_receipt(
    event: Event,
    client: &Client,
) -> Result<ZapReceipt, Box<dyn std::error::Error + Send + Sync>> {
    let mut recipient_pubkey = None;
    let mut zapper_pubkey = None; // This is us, but we get it from the 'P' tag
    let mut zapped_event_id = None;
    let mut amount_msats = 0;
    let note: String;

    let description_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Description)
        .and_then(|t| t.as_slice().get(1))
        .ok_or("Descriptionタグが見つかりません")?;

    let zap_request_event = Event::from_json(description_tag)?;
    note = zap_request_event.content.clone();

    // Check for the specific k tag ("30315")
    let required_k_tag_found =
        zap_request_event
            .tags
            .iter()
            .any(|t| {
                if t.kind() == TagKind::SingleLetter(SingleLetterTag::from_char('k').unwrap())
                {
                    if let Some(val) = t.as_slice().get(1) {
                        return val == "30315";
                    }
                }
                false
            });

    if !required_k_tag_found {
        return Err("Required k tag '30315' not found in zap request".into());
    }

    for tag in zap_request_event.tags {
        if let Some(nostr::TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
             recipient_pubkey = Some(*public_key);
        } else if let Some(nostr::TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            zapped_event_id = Some(*event_id);
        }
    }

    let recipient_pubkey = recipient_pubkey.ok_or("受信者の公開鍵が見つかりません")?;

    if let Some(bolt11_tag) = event.tags.iter().find(|t| t.kind() == TagKind::Bolt11) {
        if let Some(invoice_str) = bolt11_tag.as_slice().get(1) {
             if let Ok(invoice) = Bolt11Invoice::from_str(invoice_str) {
                if let Some(amount) = invoice.amount_milli_satoshis() {
                    amount_msats = amount;
                }
            }
        }
    }

    if let Some(p_tag) = event.tags.iter().find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::from_char('P').unwrap())) {
        if let Some(pk_str) = p_tag.as_slice().get(1) {
            zapper_pubkey = Some(nostr::PublicKey::from_str(pk_str)?);
        }
    }


    // Fetch recipient's profile
    let recipient_metadata = get_profile_metadata(recipient_pubkey, client)
        .await
        .unwrap_or_else(|_| ProfileMetadata::default());

    Ok(ZapReceipt {
        id: event.id,
        zapper_pubkey,
        recipient_pubkey,
        recipient_metadata,
        amount_msats,
        created_at: event.created_at,
        note,
        zapped_event_id,
    })
}
