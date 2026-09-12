//! 社团 OA 公共网盘服务（drive）。
//!
//! 当前实现：空间、目录/文件、上传（本地存储后端）、下载、回收站；
//! 分享链接、S3 预签名直传、分片上传在后续迭代补齐。

#![warn(missing_docs)]

/// 配置。
pub mod config {
    use std::collections::HashMap;

    use anyhow::anyhow;

    /// 运行配置。
    #[derive(Debug, Clone)]
    pub struct Config {
        /// 数据库。
        pub database_url: String,
        /// 监听地址。
        pub bind_addr: String,
        /// JWT issuer。
        pub issuer: String,
        /// 存储驱动：local（s3 后续）。
        pub storage_driver: String,
        /// 本地存储根目录。
        pub storage_local_path: String,
        /// 下载签名密钥。
        pub storage_secret: String,
    }

    impl Config {
        /// 从环境变量加载。
        pub fn from_env() -> anyhow::Result<Self> {
            Self::from_map(std::env::vars().collect())
        }

        /// 从键值映射加载（测试用）。
        pub fn from_map(map: HashMap<String, String>) -> anyhow::Result<Self> {
            Ok(Self {
                database_url: map
                    .get("DATABASE_URL")
                    .ok_or_else(|| anyhow!("缺少 DATABASE_URL"))?
                    .to_string(),
                bind_addr: map
                    .get("DRIVE_BIND_ADDR")
                    .cloned()
                    .unwrap_or_else(|| "0.0.0.0:8087".to_string()),
                issuer: map
                    .get("AUTH_ISSUER")
                    .cloned()
                    .unwrap_or_else(|| "http://localhost:8081".to_string())
                    .trim_end_matches('/')
                    .to_string(),
                storage_driver: map
                    .get("STORAGE_DRIVER")
                    .cloned()
                    .unwrap_or_else(|| "local".to_string()),
                storage_local_path: map
                    .get("STORAGE_LOCAL_PATH")
                    .cloned()
                    .unwrap_or_else(|| "data/storage".to_string()),
                storage_secret: map
                    .get("STORAGE_SECRET")
                    .cloned()
                    .unwrap_or_else(|| "dev-storage-secret".to_string()),
            })
        }
    }
}

/// 数据库连接。
pub mod db {
    use std::time::Duration;

    use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

    /// 创建 schema 并以其为 search_path 连接。
    pub async fn connect_with_schema(url: &str, schema: &str) -> Result<DatabaseConnection, DbErr> {
        // schema 名仅允许小写字母/数字/下划线，防配置注入
        if schema.is_empty()
            || !schema
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(DbErr::Custom(format!("非法 schema 名称: {schema}")));
        }
        let bootstrap = Database::connect(url).await?;
        bootstrap
            .execute_unprepared(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))
            .await?;
        bootstrap.close().await?;
        let mut options = ConnectOptions::new(url.to_string());
        options.set_schema_search_path(schema);
        options.max_connections(10);
        options.acquire_timeout(Duration::from_secs(5));
        Database::connect(options).await
    }
}

/// 应用状态。
pub mod state {
    use std::sync::{Arc, RwLock};

    use anyhow::Context;
    use jsonwebtoken::DecodingKey;
    use sea_orm::DatabaseConnection;

    use club_auth_sdk::{decode_access_token, Claims, TokenVerifier};
    use club_common::AppError;
    use club_storage::{LocalBackend, StorageBackend};

    use crate::config::Config;

    /// 状态。
    pub struct AppState {
        /// 数据库。
        pub db: DatabaseConnection,
        /// 配置。
        pub config: Config,
        /// 存储后端。
        pub storage: Arc<dyn StorageBackend>,
        /// 本地后端（签名 URL 用；S3 模式下为空）。
        pub local: Option<LocalBackend>,
        /// JWKS 解码 key。
        pub signing_key: RwLock<Option<DecodingKey>>,
    }

    impl AppState {
        /// 当前时间。
        pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc::now()
        }

        /// 加载 JWKS。
        pub async fn load_jwks(&self) -> anyhow::Result<()> {
            let url = format!("{}/.well-known/jwks.json", self.config.issuer);
            let jwks: club_auth_sdk::Jwks = reqwest::get(&url)
                .await
                .context("请求 JWKS 失败")?
                .error_for_status()?
                .json()
                .await
                .context("解析 JWKS 失败")?;
            let key = club_auth_sdk::jwks::decoding_key_from_jwks(&jwks, None)
                .map_err(|err| anyhow::anyhow!("JWKS 无可用公钥: {err}"))?;
            *self.signing_key.write().expect("lock") = Some(key);
            Ok(())
        }
    }

    impl TokenVerifier for AppState {
        fn verify_token(&self, token: &str) -> Result<Claims, AppError> {
            let guard = self.signing_key.read().expect("lock");
            let key = guard
                .as_ref()
                .ok_or_else(|| AppError::internal("JWKS 未加载"))?;
            decode_access_token(token, key, &self.config.issuer)
                .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效或已过期"))
        }
    }

    /// 共享状态。
    #[derive(Clone)]
    pub struct SharedState(Arc<AppState>);

    impl SharedState {
        /// 包装状态。
        pub fn new(state: AppState) -> Self {
            Self(Arc::new(state))
        }
    }

    impl std::ops::Deref for SharedState {
        type Target = AppState;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl TokenVerifier for SharedState {
        fn verify_token(&self, token: &str) -> Result<Claims, AppError> {
            self.0.verify_token(token)
        }
    }
}

pub mod entity;
pub mod migration;
pub mod repo;
/// HTTP 路由。
pub mod routes;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// 单文件上传大小上限（100MB；分片上传后续支持更大文件）。
pub const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

/// 构建路由。
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(routes::healthz))
        .route("/readyz", axum::routing::get(routes::readyz))
        .nest("/api/v1/drive", routes::router())
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
