use std::{io::BufRead, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Context, bail};
use axum::{
    Router,
    extract::State,
    routing::get,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, header},
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use tower_http::{
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

mod access;
mod api;
mod auth;
mod captcha;
mod cli;
mod entity;
mod error;
mod gmaps;
mod guest;
mod migration;
mod mail;
mod maintenance;
mod markdown;
mod ogp;
mod plaintext;
mod public;
mod refs;
mod sanitize;
mod seed;
mod totp;
mod tweet;
mod youtube;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub limiter: Arc<auth::LoginLimiter>,
    /// パスワードは通って、2段階認証のコード待ちのログイン
    pub mfa_pending: Arc<auth::MfaPending>,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
    /// アップロード画像の保存先（静的ファイルの配信元とは分ける）
    pub upload_dir: PathBuf,
    /// ビルド済みフロントの index.html（/p/{id} で OGP を差し込んで返す）。開発時（Vite）は None
    pub index_html: Option<Arc<String>>,
    /// ビルド済みフロントの置き場所
    pub static_dir: PathBuf,
    /// サイトごとのアイコンの置き場所（あれば同梱のものより優先する）
    pub branding_dir: PathBuf,
    /// サイト全体の設定（管理画面で変えるとここも書き換わる）
    pub site: api::site::SharedSite,
}

fn env_flag(key: &str) -> bool {
    std::env::var(key).is_ok_and(|v| matches!(v.as_str(), "1" | "true" | "yes"))
}

async fn connect() -> anyhow::Result<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL が未設定です")?;
    let mut opt = ConnectOptions::new(url);
    // 常駐メモリを抑えるため接続数は少なめ
    opt.max_connections(5).min_connections(1).sqlx_logging(false);
    let db = Database::connect(opt).await?;
    migration::Migrator::up(&db, None).await?;
    Ok(db)
}

const USAGE: &str = "\
使い方:
  brikrs [serve]                                サーバーを起動（起動時にマイグレーションも実行）
  brikrs migrate                                マイグレーションだけ実行
  brikrs maintenance                            掃除（ゴミ箱の自動削除・履歴の上限・未使用の画像）を1回実行
  brikrs create-user <username> [--admin] [--name 表示名]
                                                    ユーザー作成。パスワードは標準入力の1行目から読む
  brikrs seed                                   サンプルページ（BrikRS の紹介・使い方・書式・仕様）を追加する
                                                    （新しい DB では初回起動で自動的に入る。SEED_SAMPLES=false で無効）
  brikrs seed --update --user <username> [--dry-run]
                                                入っているサンプルページを今の版の内容で上書きする
                                                    （自分で作ったページには触れない。前の本文は編集履歴に残る。
                                                    --dry-run は何をするかの表示だけ）
  brikrs reset-totp <username>                  2段階認証を解除する（認証アプリを失くした管理者の救済用）
  brikrs page <list|get|export|post> ...         ページの読み出し・投稿（詳しくは brikrs page --help）
  brikrs mail-test                              承認待ちコメントの通知の宛先（COMMENT_NOTIFY_TO）へ試しのメールを送る
  brikrs healthcheck                            起動中のサーバーに問い合わせる（コンテナのヘルスチェック用）
  brikrs --version                              バージョンを表示";

/// システム全体のバージョン（backend/Cargo.toml が正本。frontend/package.json も同じ番号にする）
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sea_orm=warn,sqlx=warn".into()),
        )
        // syslog に流すときは色付けの制御文字を入れない
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => serve().await,
        Some("migrate") => {
            connect().await?;
            println!("マイグレーション完了");
            Ok(())
        }
        Some("maintenance") => {
            let db = connect().await?;
            let r = maintenance::run(&db, &upload_dir(), maintenance::Config::from_env())
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            println!("{r}");
            Ok(())
        }
        Some("seed") => seed_cmd(&args[1..]).await,
        Some("create-user") => create_user_cmd(&args[1..]).await,
        Some("page") => {
            if args.len() < 2 || args[1] == "--help" {
                println!("{}", cli::USAGE);
                return Ok(());
            }
            let db = connect().await?;
            cli::page(&db, &args[1..]).await
        }
        Some("reset-totp") => {
            let username = args.get(1).context(USAGE)?;
            let db = connect().await?;
            let u = api::find_user(&db, username)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?
                .with_context(|| format!("ユーザー {username} はいません"))?;
            api::clear_totp(&db, u.id)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            println!("{username} の2段階認証を解除しました（ログイン中の端末もログアウトされます）");
            Ok(())
        }
        Some("mail-test") => mail_test().await,
        Some("healthcheck") => healthcheck().await,
        Some("--version" | "-V" | "version") => {
            println!("brikrs {VERSION}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            bail!("不明なコマンドです")
        }
    }
}

async fn seed_cmd(args: &[String]) -> anyhow::Result<()> {
    let (mut update, mut dry_run, mut username) = (false, false, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--update" => update = true,
            "--dry-run" => dry_run = true,
            "--user" => username = it.next().cloned(),
            _ => bail!("{USAGE}"),
        }
    }
    let db = connect().await?;
    if !update {
        if dry_run || username.is_some() {
            bail!("--dry-run・--user は --update と一緒に指定します");
        }
        let n = seed::seed(&db, &upload_dir()).await?;
        println!("サンプルページを {n} 件作成しました");
        return Ok(());
    }
    let username = username
        .context("--user で上書きする人のユーザー名を指定してください（編集履歴に残ります）")?;
    let user = api::find_user(&db, &username)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .with_context(|| format!("ユーザー {username} はいません"))?;
    let r = seed::update(&db, &upload_dir(), user.id, dry_run).await?;
    for l in &r.lines {
        println!("{l}");
    }
    println!(
        "{}上書き {} 件・作成 {} 件・変更なし {} 件・スキップ {} 件",
        if dry_run {
            "（--dry-run：書き込んでいません）"
        } else {
            ""
        },
        r.updated,
        r.created,
        r.unchanged,
        r.skipped
    );
    Ok(())
}

/// メールの設定を確かめる（コメントを投稿しなくても試せるように）
async fn mail_test() -> anyhow::Result<()> {
    let cfg = mail::MailConfig::from_env().context("SMTP_SERVER・SMTP_USER が未設定です")?;
    let to = mail::notify_to();
    if to.is_empty() {
        bail!("COMMENT_NOTIFY_TO が未設定です");
    }
    mail::send(
        &cfg,
        &to,
        "[BrikRS] メール通知の確認",
        "BrikRS のメール通知の設定を確かめるためのメールです。\n\
         このメールが届いていれば、承認待ちのゲストコメントがあるときにも通知が届きます。\n",
    )
    .await
    .context("メールを送れませんでした")?;
    println!("{} 件の宛先へ送りました（{}:{}）", to.len(), cfg.server, cfg.port);
    Ok(())
}

/// distroless イメージには curl が無いので、自前で HTTP を1回だけ投げる
async fn healthcheck() -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let bind = std::env::var("BIND_ADDR").unwrap_or("127.0.0.1:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")),
    )
    .await??;
    stream
        .write_all(b"GET /api/me HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf).await?;
    if buf[..n].starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        bail!("unhealthy: {}", String::from_utf8_lossy(&buf[..n]))
    }
}

async fn create_user_cmd(args: &[String]) -> anyhow::Result<()> {
    let mut username = None;
    let mut display_name = None;
    let mut is_admin = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--admin" => is_admin = true,
            "--name" => display_name = it.next().cloned(),
            s if username.is_none() && !s.starts_with("--") => username = Some(s.to_string()),
            _ => bail!("{USAGE}"),
        }
    }
    let username = username.context(USAGE)?;
    let display_name = display_name.unwrap_or_else(|| username.clone());

    eprint!("パスワード: ");
    let mut pw = String::new();
    std::io::stdin().lock().read_line(&mut pw)?;
    let pw = pw.trim_end_matches(['\r', '\n']).to_string();

    let db = connect().await?;
    let u = api::create_user(&db, &username, &display_name, pw, is_admin)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!(
        "ユーザーを作成しました: id={} username={} admin={}",
        u.id, u.username, u.is_admin
    );
    Ok(())
}

fn upload_dir() -> PathBuf {
    PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or("./data/uploads".into()))
}

async fn serve() -> anyhow::Result<()> {
    let db = connect().await?;
    api::backfill_index(&db)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let upload_dir = upload_dir();
    std::fs::create_dir_all(&upload_dir)
        .with_context(|| format!("{} を作成できません", upload_dir.display()))?;

    seed::seed_if_fresh(&db, &upload_dir).await?;
    maintenance::spawn(db.clone(), upload_dir.clone(), maintenance::Config::from_env());
    ogp::log_deny_config();
    mail::log_config();

    // ビルド済みフロントエンド（無ければ API のみ。開発時は Vite が画面を出す）
    let site = api::site::SiteSettings::load(&db)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let static_dir = PathBuf::from(std::env::var("STATIC_DIR").unwrap_or("../frontend/dist".into()));
    let index_html = std::fs::read_to_string(static_dir.join("index.html"))
        .ok()
        .map(Arc::new);

    let state = AppState {
        db,
        limiter: Arc::default(),
        mfa_pending: Arc::default(),
        cookie_secure: env_flag("COOKIE_SECURE"),
        trust_proxy: env_flag("TRUST_PROXY"),
        upload_dir: upload_dir.clone(),
        index_html: index_html.clone(),
        static_dir: static_dir.clone(),
        branding_dir: PathBuf::from(
            std::env::var("BRANDING_DIR").unwrap_or("./data/branding".into()),
        ),
        site: Arc::new(std::sync::RwLock::new(site)),
    };

    // アップロード画像はランダムな名前で上書きされないので、長くキャッシュさせてよい
    let uploads = Router::new()
        .fallback_service(ServeDir::new(&upload_dir))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    let mut app = Router::new()
        .nest("/api", api::router())
        .nest("/uploads", uploads)
        // フィードと、共有用のタイトル・説明（OGP）入りの HTML
        .route("/feed.atom", get(public::site_feed))
        .route("/p/{id}/feed.atom", get(public::page_feed))
        .route("/p/{id}", get(public::page_html))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024));

    // アイコン：サイトごとの置き場所（BRANDING_DIR）にあればそれを、無ければ同梱のものを返す
    for &(name, mime) in public::BRANDING_FILES {
        app = app.route(
            &format!("/{name}"),
            get(move |State(st): State<AppState>| public::branding_file(st, name, mime)),
        );
    }

    // ビルド済みフロントエンドを配信（SPA なので未知のパスは index.html）
    if index_html.is_some() {
        tracing::info!("static files: {}", static_dir.display());
        // index.html はサイトのタイトルを差し込んで返す（"/" も ServeDir に直接返させない）
        app = app.fallback_service(
            ServeDir::new(&static_dir)
                .append_index_html_on_directories(false)
                .fallback(get(public::spa_index).with_state(state.clone())),
        );
    } else {
        tracing::warn!(
            "{} に index.html が無いため API のみ提供します（開発時は Vite を使用）",
            static_dir.display()
        );
    }

    let security_headers = [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' https: data: blob:; font-src 'self' data:; connect-src 'self'; \
             frame-src https://www.youtube-nocookie.com https://www.google.com https://platform.twitter.com; \
             object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
        ),
    ];
    for (name, value) in security_headers {
        app = app.layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from(name),
            HeaderValue::from_static(value),
        ));
    }

    let app = app.layer(TraceLayer::new_for_http()).with_state(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or("127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("BrikRS {VERSION} listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await?;
    Ok(())
}
