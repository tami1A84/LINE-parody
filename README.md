# なう.

[日本語](#n) | [English](#n-1)

**会話をなくし、シンプルなステータス共有を実現するNostrアプリケーション**

## 概要

現代のSNSは会話が中心となり、望まない会話や過剰な情報に疲れることも少なくありません。
このアプリケーションは、Twitterの原点である「〇〇なう」を共有するシンプルな体験に回帰するために作成されました。
投稿はNIP-38（`kind:30315`）を利用します。これはリプレイスブル（上書き可能）なイベントで、永続的な記録ではなく、一時的な「ステータス」の共有に最適な仕組みです。
このアプリは、リプライや「いいね」といったソーシャルな機能を取り除き、純粋なステータス共有の場を提供します。

## スクリーンショット

> **注:** スクリーンショットは現在のバージョンと異なる場合があります。ウォレットやZAP機能などの新しい機能は、これらの画像には反映されていません。

![Login Screen](images/login_screen.png)
![Home Screen](images/home_screen.png)
![Post Screen](images/post_screen.png)
![Relays Screen](images/relays_screen.png)
![Profile Screen](images/profile_screen.png)

## 特徴

*   **洗練されたUI:** `egui`とLINE Seed JPフォントを採用し、モダンなデザインで、ライトモードとダークモードの両方に対応しています。
*   **多彩なステータス投稿 (NIP-38):** 一般的なステータスのほか、「音楽」「ポッドキャスト」など、種類に応じた専用の投稿が可能です。これは上書き可能なイベントのため、常に最新の状況を共有できます。
*   **カスタム絵文字対応 (NIP-30):** あなたのNostrプロファイルで設定したカスタム絵文字を投稿に利用できます。
*   **Nostr Wallet Connect & ZAP (NIP-47, NIP-57):** Nostr Wallet Connect（NWC）に対応し、ウォレットを安全に接続できます。タイムライン上の投稿に対してZAP（投げ銭）を送信し、感謝や応援の気持ちを伝えることができます。接続情報はアプリのパスフレーズで暗号化され、安全に保管されます。
*   **プロフィールの表示と編集 (NIP-01):** Nostrのプロフィール情報（lud16のライトニングアドレスを含む）を表示し、編集することができます。
*   **安全な鍵管理 (NIP-49):** 秘密鍵はローカルに保存されます。あなたのパスフレーズからPBKDF2で導出された鍵を使い、ChaCha20Poly1305で暗号化されるため安全です。
*   **高度なリレー管理と投稿取得 (NIP-65, NIP-02):**
    *   **あなたのリレー:** ログイン時にあなたのNIP-65リレーリストに自動接続します。リストがない場合はデフォルトリレーを使用します。
    *   **投稿の取得:** フォローしているユーザー(NIP-02)のNIP-65リレーリストを別途取得し、そこからステータス投稿を検索することで、取りこぼしの少ないタイムラインを実現します。
    *   **リレーリストの編集:** アプリ内からリレーの追加・削除、読み書き権限の設定、NIP-65リストの公開が可能です。
*   **効率的なキャッシュとデータ移行:** プロフィール、フォローリスト、リレーリストなどをLMDBにキャッシュし、高速なデータ表示を実現します。旧バージョンからの移行時には、古いファイルベースのキャッシュから自動でデータを引き継ぎます。
*   **タブ形式のインターフェース:** ホーム（タイムラインと投稿）、リレー、ウォレット、プロフィールのタブで簡単に機能を切り替えられます。
*   **会話よりステータス共有を重視:** リプライ、メンション、リアクションといった会話機能は意図的に排除されています。ただし、ZAP（NIP-57）による感謝の表現はサポートしています。

## 技術スタック

*   **言語:** [Rust](https://www.rust-lang.org/)
*   **GUI:** [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui)
*   **Nostrプロトコル:** [nostr-sdk](https://github.com/nostr-protocol/nostr-sdk), [nostr](https://github.com/rust-nostr/nostr) (NIP-47, NIP-57対応)
*   **非同期処理:** [Tokio](https://tokio.rs/)
*   **HTTPクライアント:** [ureq](https://github.com/algesten/ureq) (LNURLリクエスト用)
*   **データベース:** [LMDB](https://www.symas.com/lmdb) (via [heed](https://github.com/meilisearch/heed))
*   **暗号化:** [chacha20poly1305](https://crates.io/crates/chacha20poly1305), [pbkdf2](https://crates.io/crates/pbkdf2)

## インストール & 使い方

1.  **リポジトリをクローンし、ディレクトリに移動します:**
    ```bash
    git clone https://github.com/tami1A84/now..git
    cd now.
    ```
2.  **アプリケーションを実行します:**
    ```bash
    cargo run
    ```
    **本番環境向けに最適化されたビルドを実行する場合は、次のコマンドを使用します:**
    ```bash
    cargo run --release
    ```
3.  **GUIウィンドウが開きます。画面の指示に従って、初回設定とステータス投稿を行ってください。**

    > **リレーに関する注記 (NIP-65):**
    > もしあなたがNIP-65でリレーリストを公開している場合、アプリケーションは自動的にそのリレーを使用します。公開していない場合は、デフォルトのリレーに接続されます。

---

# now.

[日本語](#n) | [English](#n-1)

**A simple Nostr application for sharing your status, not for conversation.**

## Abstract

Modern social networks are centered around conversation, often leading to information overload and unwanted interactions.
This application is a return to the simple "What are you doing?" experience of early Twitter.
It uses NIP-38 (`kind:30315`), a replaceable event ideal for sharing temporary "statuses" rather than permanent posts.
This app removes social features like replies and likes to provide a pure status-sharing platform.

## Screenshot

> **Note:** Screenshots may differ from the current version. New features like the Wallet and Zapping are not reflected in these images.

![Login Screen](images/login_screen.png)
![Home Screen](images/home_screen.png)
![Post Screen](images/post_screen.png)
![Relays Screen](images/relays_screen.png)
![Profile Screen](images/profile_screen.png)

## Features

*   **Sophisticated UI:** A modern design using `egui` and the LINE Seed JP font, with both light and dark modes.
*   **Versatile Status Posts (NIP-38):** In addition to general updates, you can post specialized statuses for "Music" and "Podcasts." As a replaceable event, you can always share your latest update.
*   **Custom Emoji Support (NIP-30):** Use custom emojis defined in your Nostr profile in your posts.
*   **Nostr Wallet Connect & Zapping (NIP-47, NIP-57):** Securely connect your wallet using Nostr Wallet Connect (NWC). Send zaps to posts on the timeline to show appreciation and support. Your NWC connection details are encrypted with your main app passphrase and stored securely.
*   **Profile Display and Editing (NIP-01):** View and edit your Nostr profile information, including your lud16 lightning address for receiving zaps.
*   **Secure Key Management (NIP-49):** Your secret key is stored locally and securely, encrypted with ChaCha20Poly1305 using a key derived from your passphrase via PBKDF2.
*   **Advanced Relay Management & Post Fetching (NIP-65, NIP-02):**
    *   **Your Relays:** Automatically connects to your NIP-65 relay list on login, or falls back to default relays if none is found.
    *   **Post Fetching:** Achieves a more complete timeline by fetching the NIP-65 relay lists of users you follow (NIP-02) and searching for their statuses there.
    *   **Relay List Editing:** Add, remove, set read/write permissions, and publish your NIP-65 list directly from within the app.
*   **Efficient Caching & Data Migration:** Caches profiles, follow lists, relay lists, and more in a local LMDB database for faster performance. It also automatically migrates data from the old file-based cache for users updating from a previous version.
*   **Tabbed Interface:** Easily switch between functions with tabs for Home (Timeline & Posting), Relays, Wallet, and Profile.
*   **Emphasis on Status Sharing over Conversation:** Conversational features like replies, mentions, and reactions are intentionally excluded. However, it supports showing appreciation through Zaps (NIP-57).

## Technical Stacks

*   **Language:** [Rust](https://www.rust-lang.org/)
*   **GUI:** [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui)
*   **Nostr Protocol:** [nostr-sdk](https://github.com/nostr-protocol/nostr-sdk), [nostr](https://github.com/rust-nostr/nostr) (with NIP-47 & NIP-57 support)
*   **Asynchronous Runtime:** [Tokio](https://tokio.rs/)
*   **HTTP Client:** [ureq](https://github.com/algesten/ureq) (for LNURL requests)
*   **Database:** [LMDB](https://www.symas.com/lmdb) (via [heed](https://github.com/meilisearch/heed))
*   **Cryptography:** [chacha20poly1305](https://crates.io/crates/chacha20poly1305), [pbkdf2](https://crates.io/crates/pbkdf2)

## Installation & Usage

1.  **Clone the repository and navigate to the directory:**
    ```bash
    git clone https://github.com/tami1A84/now..git
    cd now.
    ```
2.  **Run the application:**
    ```bash
    cargo run
    ```
    **To execute a build optimized for production environments, use the following command:**
    ```bash
    cargo run --release
    ```
4.  **The GUI window will open. Follow the on-screen instructions for setup and status posting.**

    > **Note on Relays (NIP-65):**
    > If you have published a relay list with NIP-65, the application will automatically use those relays for posting. If not, it will connect to default relays.
