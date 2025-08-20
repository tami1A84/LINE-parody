mod cache_db;
mod emoji_loader;
mod nip49;
mod nostr_client;
mod ui;
mod types;

use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use std::fs;
use nostr::PublicKey;
use regex::Regex;

mod theme;

use crate::cache_db::{LmdbCache, DB_FOLLOWED, DB_PROFILES, DB_RELAYS};
use crate::types::*;


const CONFIG_FILE: &str = "config.json"; // 設定ファイル名

const DB_PATH: &str = "cache_db";
const CACHE_DIR: &str = "cache"; // Re-added for migration

const MAX_STATUS_LENGTH: usize = 140; // ステータス最大文字数

async fn migrate_data_from_files(
    cache_db: &LmdbCache,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cache_path = Path::new(CACHE_DIR);
    if !cache_path.exists() {
        return Ok(());
    }

    println!("Old cache directory found. Starting data migration...");

    let mut files_by_pubkey: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
    let re = Regex::new(r"([a-f0-9]{64})_.*\.json")?;

    for entry in fs::read_dir(cache_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(captures) = re.captures(path.file_name().unwrap().to_str().unwrap()) {
                if let Some(pubkey) = captures.get(1) {
                    files_by_pubkey
                        .entry(pubkey.as_str().to_string())
                        .or_default()
                        .push(path);
                }
            }
        }
    }

    for (pubkey_hex, paths) in files_by_pubkey {
        println!("Migrating data for pubkey: {}", pubkey_hex);
        for path in paths {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if file_name.ends_with("_followed.json") {
                let content = fs::read_to_string(&path)?;
                let cache: Cache<HashSet<PublicKey>> = serde_json::from_str(&content)?;
                cache_db.write_cache(DB_FOLLOWED, &pubkey_hex, &cache.data)?;
                println!("  - Migrated followed list.");
            } else if file_name.ends_with("_nip65.json") {
                let content = fs::read_to_string(&path)?;
                let cache: Cache<Vec<(String, Option<String>)>> = serde_json::from_str(&content)?;
                cache_db.write_cache(DB_RELAYS, &pubkey_hex, &cache.data)?;
                println!("  - Migrated NIP-65 relays.");
            } else if file_name.ends_with("_profile.json") {
                let content = fs::read_to_string(&path)?;
                let cache: Cache<ProfileMetadata> = serde_json::from_str(&content)?;
                cache_db.write_cache(DB_PROFILES, &pubkey_hex, &cache.data)?;
                println!("  - Migrated profile metadata.");
            }
        }
    }

    // Rename the old cache directory to prevent re-migration
    let migrated_path = Path::new("cache_migrated");
    fs::rename(cache_path, migrated_path)?;
    println!("Data migration complete. Old cache directory renamed to 'cache_migrated'.");

    Ok(())
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

        // --- フォント設定 ---
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "LINESeedJP".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/fonts/LINESeedJP_TTF_Rg.ttf"))
                .into(),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "LINESeedJP".to_owned());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("LINESeedJP".to_owned());

        _cc.egui_ctx.set_fonts(fonts);

        // --- スタイル調整 ---
        style.visuals = theme::light_visuals(); // ライトモードを基準にする

        // 角丸やテキストスタイルは共通で設定
        let corner_radius = 6.0;
        style.visuals.widgets.noninteractive.corner_radius = corner_radius.into();
        style.visuals.widgets.inactive.corner_radius = corner_radius.into();
        style.visuals.widgets.hovered.corner_radius = corner_radius.into();
        style.visuals.widgets.active.corner_radius = corner_radius.into();
        style.visuals.widgets.open.corner_radius = corner_radius.into();

        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(12.0, egui::FontFamily::Monospace),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();

        _cc.egui_ctx.set_style(style);

        let lmdb_cache =
            LmdbCache::new(Path::new(DB_PATH)).expect("Failed to initialize LMDB cache");

        let app_data_internal = NostrStatusAppInternal {
            nwc_uri_input: String::new(),
            cache_db: lmdb_cache,
            is_logged_in: false,
            status_message_input: String::new(),
            show_post_dialog: false,
            show_emoji_picker: false,
            my_emojis: HashMap::new(),
            secret_key_input: String::new(),
            passphrase_input: String::new(),
            confirm_passphrase_input: String::new(),
    current_status_type: StatusType::General,
    show_music_dialog: false,
    music_track_input: String::new(),
    music_url_input: String::new(),
    show_podcast_dialog: false,
    podcast_episode_input: String::new(),
    podcast_url_input: String::new(),
            nostr_client: None,
            my_keys: None,
            followed_pubkeys: HashSet::new(),
            followed_pubkeys_display: String::new(),
            timeline_posts: Vec::new(),
            should_repaint: false,
            is_loading: false,
            current_tab: AppTab::Home,
            connected_relays_display: String::new(),
            nip01_profile_display: String::new(), // ここを初期化
            editable_profile: ProfileMetadata::default(), // 編集可能なプロファイルデータ
            profile_fetch_status: "Fetching profile...".to_string(), // プロファイル取得状態
            // リレーリスト編集用のフィールドを初期化
            nip65_relays: Vec::new(),
            discover_relays_editor: "wss://purplepag.es\nwss://directory.yabu.me".to_string(),
            default_relays_editor:
                "wss://relay.damus.io\nwss://relay.nostr.wirednet.jp\nwss://yabu.me".to_string(),
            current_theme: AppTheme::Light,
            image_cache: HashMap::new(),
            nwc_passphrase_input: String::new(),
            nwc: None,
            nwc_client: None,
            nwc_error: None,
            zap_history: Vec::new(),
            zap_history_fetch_status: String::new(),
            is_fetching_zap_history: false,
            show_zap_dialog: false,
            zap_amount_input: String::new(),
            zap_target_post: None,
        };
        let data = Arc::new(Mutex::new(app_data_internal));

        // egui_extrasの画像ローダーをインストール
        egui_extras::install_image_loaders(&_cc.egui_ctx);

        // アプリケーション起動時にデータ移行と設定ファイルチェック
        let data_clone = data.clone();
        let runtime_handle = runtime.handle().clone();

        runtime_handle.spawn(async move {
            // Run migration
            let cache_db_clone = {
                let app_data = data_clone.lock().unwrap();
                app_data.cache_db.clone()
            };
            if let Err(e) = migrate_data_from_files(&cache_db_clone).await {
                eprintln!("Data migration failed: {e}");
            }

            let mut app_data = data_clone.lock().unwrap();
            // println!("Checking config file...");

            if Path::new(CONFIG_FILE).exists() {
                // println!("Existing user: Please enter your passphrase.");
            } else {
                // println!("First-time setup: Enter your secret key and set a passphrase.");
            }
            app_data.should_repaint = true;
        });

        Self { data, runtime }
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init(); // 必要に応じて有効化

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "N",
        options,
        Box::new(|cc| Ok(Box::new(NostrStatusApp::new(cc)))),
    )
}
