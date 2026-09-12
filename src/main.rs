//! drive 服务入口。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Context;
use sea_orm_migration::MigratorTrait;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use club_storage::{LocalBackend, S3Backend, StorageBackend};
use drive_service::config::Config;
use drive_service::migration::Migrator;
use drive_service::state::{AppState, SharedState};
use drive_service::{build_router, db};

/// 初始化日志。
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

/// 加载 JWKS（重试）。
async fn load_jwks_with_retry(state: &AppState) -> anyhow::Result<()> {
    let mut last_error = None;
    for attempt in 1..=10 {
        match state.load_jwks().await {
            Ok(()) => {
                tracing::info!(attempt, "JWKS 加载成功");
                return Ok(());
            }
            Err(err) => {
                tracing::warn!(attempt, error = %err, "JWKS 加载失败，1 秒后重试");
                last_error = Some(err);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("JWKS 加载失败")))
}

/// 程序入口。
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = Config::from_env()?;
    tracing::info!(bind = %config.bind_addr, driver = %config.storage_driver, "drive 服务启动中");

    let database = db::connect_with_schema(&config.database_url, "drive")
        .await
        .context("连接数据库失败")?;
    Migrator::up(&database, None)
        .await
        .context("数据库迁移失败")?;

    let (storage, local): (Arc<dyn StorageBackend>, Option<LocalBackend>) =
        match config.storage_driver.as_str() {
            "local" => {
                let backend =
                    LocalBackend::new(&config.storage_local_path, config.storage_secret.clone());
                (Arc::new(backend.clone()), Some(backend))
            }
            "s3" => {
                let endpoint = config
                    .s3_endpoint
                    .clone()
                    .context("driver=s3 需要 S3_ENDPOINT")?;
                let bucket = config
                    .s3_bucket
                    .clone()
                    .context("driver=s3 需要 S3_BUCKET")?;
                let access_key = config
                    .s3_access_key
                    .clone()
                    .context("driver=s3 需要 S3_ACCESS_KEY")?;
                let secret_key = config
                    .s3_secret_key
                    .clone()
                    .context("driver=s3 需要 S3_SECRET_KEY")?;
                let backend = S3Backend::new(
                    &endpoint,
                    &config.s3_region,
                    &bucket,
                    &access_key,
                    &secret_key,
                )
                .context("初始化 S3 后端失败")?;
                (Arc::new(backend), None)
            }
            other => anyhow::bail!("不支持的存储驱动: {other}（仅 local/s3）"),
        };
    let state = SharedState::new(AppState {
        db: database,
        config,
        storage,
        local,
        signing_key: RwLock::new(None),
    });
    load_jwks_with_retry(&state).await?;

    let listener = TcpListener::bind(&state.config.bind_addr)
        .await
        .with_context(|| format!("监听 {} 失败", state.config.bind_addr))?;
    tracing::info!(addr = %state.config.bind_addr, "HTTP 服务已就绪");
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("HTTP 服务异常退出")?;
    Ok(())
}
