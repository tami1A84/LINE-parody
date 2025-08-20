pub mod login_view;
pub mod home_view;
pub mod relays_view;
pub mod profile_view;
pub mod wallet_view;
pub mod image_cache;
pub mod zap;

use eframe::egui::{self, Margin};
// nostr v0.43.0 / nostr-sdk: RelayMetadata は nostr_sdk::nips::nip65 に移動したため import する
use crate::{
    NostrStatusApp,
    theme::{dark_visuals, light_visuals},
    types::*,
};

impl eframe::App for NostrStatusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut app_data = self.data.lock().unwrap();

        let home_tab_text = "ホーム";
        let relays_tab_text = "リレー";
        let wallet_tab_text = "ウォレット";
        let profile_tab_text = "プロフィール";

        // app_data_arc をクローンして非同期タスクに渡す
        let app_data_arc_clone = self.data.clone();
        let runtime_handle = self.runtime.handle().clone();

        let panel_frame = egui::Frame::default()
            .inner_margin(Margin::same(15))
            .fill(ctx.style().visuals.panel_fill);

        egui::SidePanel::left("side_panel")
            .frame(panel_frame)
            .min_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);

                ui.horizontal(|ui| {
                    ui.heading("なう");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (icon, new_theme) = match app_data.current_theme {
                            AppTheme::Light => ("☀️", AppTheme::Dark),
                            AppTheme::Dark => ("🌙", AppTheme::Light),
                        };
                        if ui.button(icon).clicked() {
                            app_data.current_theme = new_theme;
                            let new_visuals = match new_theme {
                                AppTheme::Light => light_visuals(),
                                AppTheme::Dark => dark_visuals(),
                            };
                            ctx.set_visuals(new_visuals);
                        }
                    });
                });

                ui.add_space(15.0);

                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    ui.style_mut().spacing.item_spacing.y = 12.0; // ボタン間の垂直スペース

                    ui.selectable_value(&mut app_data.current_tab, AppTab::Home, home_tab_text);
                    if app_data.is_logged_in {
                        ui.selectable_value(
                            &mut app_data.current_tab,
                            AppTab::Relays,
                            relays_tab_text,
                        );
                        ui.selectable_value(
                            &mut app_data.current_tab,
                            AppTab::Wallet,
                            wallet_tab_text,
                        );
                        ui.selectable_value(
                            &mut app_data.current_tab,
                            AppTab::Profile,
                            profile_tab_text,
                        );
                    }
                });

                if app_data.is_logged_in {
                    ui.add_space(20.0);

                    // --- 投稿ボタン ---
                    let post_button_text = egui::RichText::new("投稿する").size(14.0).strong();
                    let button = egui::Button::new(post_button_text)
                        .min_size(egui::vec2(ui.available_width(), 40.0))
                        .corner_radius(egui::CornerRadius::from(8.0));

                    if ui.add(button).clicked() {
                        app_data.show_post_dialog = true;
                    }
                }
            });

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {

            // ui.add_enabled_ui(!app_data.is_loading, |ui| { // この行を削除
                if !app_data.is_logged_in {
                    if app_data.current_tab == AppTab::Home {
                        login_view::draw_login_view(ui, &mut app_data, app_data_arc_clone, runtime_handle);
                    }
                } else {
                    match app_data.current_tab {
                        AppTab::Home => {
                            home_view::draw_home_view(ui, ctx, &mut app_data, app_data_arc_clone, runtime_handle);
                        },
                        AppTab::Relays => {
                           relays_view::draw_relays_view(ui, &mut app_data, app_data_arc_clone, runtime_handle);
                        },
                        AppTab::Wallet => {
                            wallet_view::draw_wallet_view(ui, &mut app_data, app_data_arc_clone, runtime_handle);
                        },
                        AppTab::Profile => {
                            profile_view::draw_profile_view(ui, ctx, &mut app_data, app_data_arc_clone, runtime_handle);
                        },
                    }
                }
            // }); // この閉じ括弧も削除
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
