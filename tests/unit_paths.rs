//! 补充覆盖：配置/数据库连接辅助/JWKS 加载与验签路径。

mod common;

use std::sync::RwLock;

use common::*;
use drive_service::state::{AppState, SharedState};

#[tokio::test]
async fn config_and_connect_validation_paths() {
    // schema 名非法 → 直接拒绝
    let err = drive_service::db::connect_with_schema(&test_database_url(), "Bad-Name").await;
    assert!(err.is_err());

    // 缺少 DATABASE_URL → 报错
    assert!(drive_service::config::Config::from_map(Default::default()).is_err());

    // from_env 正常路径
    std::env::set_var("DATABASE_URL", test_database_url());
    let config = drive_service::config::Config::from_env().expect("env config");
    assert!(!config.issuer.is_empty());
    assert_eq!(config.storage_driver, "local");
    std::env::remove_var("DATABASE_URL");
}

#[tokio::test]
async fn jwks_load_and_verify_paths() {
    let app = spawn().await;

    // 用真实公钥构造 JWKS，并由本地 stub 提供
    use rsa::pkcs8::DecodePublicKey;
    use rsa::traits::PublicKeyParts;
    let public = rsa::RsaPublicKey::from_public_key_pem(
        std::str::from_utf8(&app.public_pem).expect("pem utf8"),
    )
    .expect("parse public");
    let jwk = club_auth_sdk::jwk_from_rsa_public_components(
        &club_auth_sdk::jwks::key_id_from_pem(&app.public_pem),
        &public.n().to_bytes_be(),
        &public.e().to_bytes_be(),
    );
    let jwks = club_auth_sdk::Jwks { keys: vec![jwk] };
    let jwks_value = serde_json::to_value(&jwks).expect("jwks json");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let value_for_route = jwks_value.clone();
    let server = axum::Router::new().route(
        "/.well-known/jwks.json",
        axum::routing::get(move || {
            let value = value_for_route.clone();
            async move { axum::Json(value) }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, server).await.expect("serve");
    });

    let mut config = app.state.config.clone();
    config.issuer = format!("http://{addr}");
    let state = SharedState::new(AppState {
        db: app.state.db.clone(),
        config: config.clone(),
        storage: app.state.storage.clone(),
        local: app.state.local.clone(),
        signing_key: RwLock::new(None),
    });

    use club_auth_sdk::TokenVerifier;
    // 未加载 JWKS 时失败
    assert!(state.verify_token("anything").is_err());
    // 加载后验签成功
    state.load_jwks().await.expect("load jwks");
    let claims = club_auth_sdk::Claims {
        sub: uuid::Uuid::now_v7().to_string(),
        name: "u".into(),
        avatar: None,
        roles: vec![],
        scopes: vec![],
        guest: false,
        iss: config.issuer.clone(),
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 60,
        jti: uuid::Uuid::now_v7().to_string(),
    };
    let kid = club_auth_sdk::jwks::key_id_from_pem(&app.public_pem);
    let token = club_auth_sdk::encode_access_token(&claims, &app.private_pem, &kid).expect("sign");
    assert_eq!(state.verify_token(&token).expect("verify").name, "u");
    // 坏令牌
    assert!(state.verify_token("bad").is_err());
}
