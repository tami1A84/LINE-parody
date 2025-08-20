use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nostr::{EventBuilder, Kind, Tag};
use nostr_sdk::{RelayUrl, nips::nip65::RelayMetadata, Client, ClientOptions as Options};

use crate::{
    types::*,
    nostr_client::{connect_to_relays_with_nip65},
    cache_db::{DB_RELAYS},
};

pub fn draw_relays_view(
    ui: &mut egui::Ui,
    app_data: &mut NostrStatusAppInternal,
    app_data_arc: Arc<Mutex<NostrStatusAppInternal>>,
    runtime_handle: tokio::runtime::Handle,
) {
    let current_connection_heading_text = "現在の接続";
    let reconnect_button_text = "再接続";
    let edit_relay_lists_heading_text = "リレーリストを編集";
    let nip65_relay_list_label_text = "あなたのリレーリスト (NIP-65)";
    let add_relay_button_text = "リレーを追加";
    let read_checkbox_text = "読み取り";
    let write_checkbox_text = "書き込み";
    let discover_relays_label_text = "発見リレー (他ユーザーを見つけるため)";
    let default_relays_label_text = "デフォルトリレー (フォールバック用)";
    let save_nip65_button_text = "保存して発見リレーに公開";

    let card_frame = egui::Frame {
        inner_margin: egui::Margin::same(12),
        corner_radius: 8.0.into(),
        shadow: eframe::epaint::Shadow::NONE,
        fill: app_data.current_theme.card_background_color(),
        ..Default::default()
    };

    egui::ScrollArea::vertical().id_salt("relays_tab_scroll_area").show(ui, |ui| {
        // --- 現在の接続状態 ---
        card_frame.show(ui, |ui| {
            ui.heading(current_connection_heading_text);
            ui.add_space(10.0);
            let reconnect_button = egui::Button::new(egui::RichText::new(reconnect_button_text).strong());
            if ui.add_enabled(!app_data.is_loading, reconnect_button).clicked() {
                let client_clone = app_data.nostr_client.as_ref().unwrap().clone();
                let keys_clone = app_data.my_keys.clone().unwrap();
                let discover_relays = app_data.discover_relays_editor.clone();
                let default_relays = app_data.default_relays_editor.clone();
                let cache_db_clone = app_data.cache_db.clone();

                app_data.is_loading = true;
                app_data.should_repaint = true;

                let cloned_app_data_arc = app_data_arc.clone();
                runtime_handle.spawn(async move {
                    match connect_to_relays_with_nip65(&client_clone, &keys_clone, &discover_relays, &default_relays).await {
                        Ok((log_message, fetched_nip65_relays)) => {
                            println!("Relay connection successful!\n{log_message}");
                            let pubkey_hex = keys_clone.public_key().to_string();
                            if let Err(e) = cache_db_clone.write_cache(DB_RELAYS, &pubkey_hex, &fetched_nip65_relays) {
                                eprintln!("Failed to write NIP-65 cache: {e}");
                            }

                            let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                            if let Some(pos) = log_message.find("--- 現在接続中のリレー ---") {
                                app_data_async.connected_relays_display = log_message[pos..].to_string();
                            }
                            app_data_async.nip65_relays = fetched_nip65_relays.into_iter().map(|(url, policy)| {
                                let (read, write) = match policy.as_deref() {
                                    Some("read") => (true, false),
                                    Some("write") => (false, true),
                                    _ => (true, true),
                                };
                                EditableRelay { url, read, write }
                            }).collect();
                        }
                        Err(e) => {
                            eprintln!("Failed to connect to relays: {e}");
                        }
                    }
                    let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                    app_data_async.is_loading = false;
                    app_data_async.should_repaint = true;
                });
            }
            ui.add_space(10.0);
            egui::ScrollArea::vertical().id_salt("relay_connection_scroll_area").max_height(150.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut app_data.connected_relays_display)
                    .desired_width(ui.available_width())
                    .interactive(false));
            });
        });

        ui.add_space(15.0);

        // --- リレーリスト編集 ---
        card_frame.show(ui, |ui| {
            ui.heading(edit_relay_lists_heading_text);
            ui.add_space(15.0);
            ui.label(nip65_relay_list_label_text);
            ui.add_space(5.0);

            let mut relay_to_remove = None;
            egui::ScrollArea::vertical().id_salt("nip65_editor_scroll").max_height(150.0).show(ui, |ui| {
                for (i, relay) in app_data.nip65_relays.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        let text_edit = egui::TextEdit::singleline(&mut relay.url).desired_width(300.0);
                        ui.add(text_edit);
                        ui.checkbox(&mut relay.read, read_checkbox_text);
                        ui.checkbox(&mut relay.write, write_checkbox_text);
                        if ui.button("❌").clicked() {
                            relay_to_remove = Some(i);
                        }
                    });
                }
            });

            if let Some(i) = relay_to_remove {
                app_data.nip65_relays.remove(i);
            }

            if ui.button(add_relay_button_text).clicked() {
                app_data.nip65_relays.push(EditableRelay::default());
            }

            ui.add_space(15.0);
            ui.label(discover_relays_label_text);
            ui.add_space(5.0);
             egui::ScrollArea::vertical().id_salt("discover_editor_scroll").max_height(80.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut app_data.discover_relays_editor)
                    .desired_width(ui.available_width()));
            });

            ui.add_space(15.0);
            ui.label(default_relays_label_text);
            ui.add_space(5.0);
            egui::ScrollArea::vertical().id_salt("default_editor_scroll").max_height(80.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut app_data.default_relays_editor)
                    .desired_width(ui.available_width()));
            });

            ui.add_space(15.0);
            let save_nip65_button = egui::Button::new(egui::RichText::new(save_nip65_button_text).strong());
            if ui.add_enabled(!app_data.is_loading, save_nip65_button).clicked() {
                let keys = app_data.my_keys.clone().unwrap();
                let nip65_relays = app_data.nip65_relays.clone();
                let discover_relays = app_data.discover_relays_editor.clone();

                app_data.is_loading = true;
                app_data.should_repaint = true;

                let cloned_app_data_arc = app_data_arc.clone();
                runtime_handle.spawn(async move {
                    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                        let tags: Vec<Tag> = nip65_relays
                            .iter()
                            .filter_map(|relay| {
                                if relay.url.trim().is_empty() {
                                    return None;
                                }
                                let policy = if relay.read && !relay.write {
                                    Some(RelayMetadata::Read)
                                } else if !relay.read && relay.write {
                                    Some(RelayMetadata::Write)
                                } else {
                                    None
                                };
                                match RelayUrl::parse(&relay.url) {
                                    Ok(url) => Some(Tag::relay_metadata(url, policy)),
                                    Err(_) => None,
                                }
                            })
                            .collect();

                        if tags.is_empty() {
                                    println!("Warning: Publishing an empty NIP-65 list.");
                        }

                        let event = EventBuilder::new(Kind::RelayList, "").tags(tags).sign(&keys).await?;

                         let opts = Options::new();
                         let discover_client = Client::builder()
                             .signer(keys.clone())
                             .opts(opts)
                             .build();
                        discover_client.connect().await;
                        discover_client.wait_for_connection(Duration::from_secs(20)).await;

                        for relay_url in discover_relays.lines() {
                            if !relay_url.trim().is_empty() {
                                discover_client.add_relay(relay_url.trim()).await?;
                            }
                        }
                        discover_client.connect().await;

                                let event_id = discover_client.send_event(&event).await?;
                                println!("NIP-65 list published with event id: {event_id:?}");

                        discover_client.shutdown().await;
                        Ok(())
                    }.await;

                    if let Err(e) = result {
                        eprintln!("Failed to publish NIP-65 list: {e}");
                    }

                    let mut app_data_async = cloned_app_data_arc.lock().unwrap();
                    app_data_async.is_loading = false;
                    app_data_async.should_repaint = true;
                });
            }
        });
    });
}
