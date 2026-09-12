//! drive 集成测试：空间/文件夹/上传下载/回收站 + 分享链接。

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use uuid::Uuid;

async fn create_space(app: &TestApp, token: &str) -> String {
    let response = request(
        &app.app,
        "POST",
        "/api/v1/drive/spaces",
        Some(token),
        Some(&json!({ "name": "项目资料", "type": "team" })),
    )
    .await;
    response.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn folder_upload_download_trash_flow() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    // 新建文件夹
    let folder = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/folders"),
        Some(&token),
        Some(&json!({ "name": "素材" })),
    )
    .await;
    let folder = folder.expect(StatusCode::CREATED);
    let folder_id = folder["id"].as_str().unwrap();

    // 上传文件到文件夹
    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=logo.png&mime=image/png&parentId={folder_id}"),
        &token,
        "image/png",
        b"fake-png-bytes",
    )
    .await;
    let uploaded = uploaded.expect(StatusCode::CREATED);
    let file_id = uploaded["id"].as_str().unwrap().to_string();
    assert_eq!(uploaded["size"], 14);

    // 目录列表
    let listing = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes?parentId={folder_id}"),
        Some(&token),
        None,
    )
    .await;
    let listing = listing.expect(StatusCode::OK);
    assert_eq!(listing.as_array().unwrap().len(), 1);
    assert_eq!(listing[0]["name"], "logo.png");

    // 下载
    let download = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/download"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.bytes, b"fake-png-bytes");

    // 签名 ticket
    let ticket = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/ticket"),
        Some(&token),
        None,
    )
    .await;
    let ticket = ticket.expect(StatusCode::OK);
    assert!(ticket["url"].as_str().unwrap().contains("signature="));

    // 删除进回收站
    let deleted = request(
        &app.app,
        "DELETE",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}"),
        Some(&token),
        None,
    )
    .await;
    deleted.expect(StatusCode::NO_CONTENT);
    let trash = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes?trashed=true"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(trash.expect(StatusCode::OK).as_array().unwrap().len(), 1);

    // 恢复
    let restored = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/restore"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored.expect(StatusCode::OK)["name"], "logo.png");
}

#[tokio::test]
async fn share_link_lifecycle_with_limits() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=report.pdf"),
        &token,
        "application/pdf",
        b"pdf-content",
    )
    .await;
    let file_id = uploaded.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 创建限一次下载的分享
    let share = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/shares"),
        Some(&token),
        Some(&json!({ "maxDownloads": 1 })),
    )
    .await;
    let share = share.expect(StatusCode::CREATED);
    let share_token = share["token"].as_str().unwrap().to_string();
    assert!(share["url"].as_str().unwrap().starts_with("/s/"));

    // 公开信息（无登录）
    let info = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}"),
        None,
        None,
    )
    .await;
    assert_eq!(info.expect(StatusCode::OK)["name"], "report.pdf");

    // 第一次下载成功
    let first = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}/download"),
        None,
        None,
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.bytes, b"pdf-content");

    // 超过次数
    let second = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}/download"),
        None,
        None,
    )
    .await;
    second.expect(StatusCode::FORBIDDEN);

    // 吊销后信息不可见
    let share_id = share["id"].as_str().unwrap();
    let revoke = request(
        &app.app,
        "DELETE",
        &format!("/api/v1/drive/spaces/{space_id}/shares/{share_id}"),
        Some(&token),
        None,
    )
    .await;
    revoke.expect(StatusCode::NO_CONTENT);
    let invalid = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}"),
        None,
        None,
    )
    .await;
    invalid.expect(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn permission_and_validation_errors() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let outsider = Uuid::now_v7();
    let token_owner = issue_token(&app, owner);
    let token_outsider = issue_token(&app, outsider);
    let space_id = create_space(&app, &token_owner).await;

    // 非成员访问
    let denied = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes"),
        Some(&token_outsider),
        None,
    )
    .await;
    denied.expect(StatusCode::FORBIDDEN);

    // 非法文件夹名
    let bad_name = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/folders"),
        Some(&token_owner),
        Some(&json!({ "name": "a/b" })),
    )
    .await;
    bad_name.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 父目录是文件 → 422
    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=note.txt"),
        &token_owner,
        "text/plain",
        b"hi",
    )
    .await;
    let file_id = uploaded.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bad_parent = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/folders"),
        Some(&token_owner),
        Some(&json!({ "name": "sub", "parentId": file_id })),
    )
    .await;
    bad_parent.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 未登录
    request(&app.app, "GET", "/api/v1/drive/spaces", None, None)
        .await
        .expect(StatusCode::UNAUTHORIZED);

    // 不存在的分享
    let missing = request(
        &app.app,
        "GET",
        "/api/v1/drive/public/shares/nope",
        None,
        None,
    )
    .await;
    missing.expect(StatusCode::NOT_FOUND);

    // 健康检查
    let health = request(&app.app, "GET", "/healthz", None, None).await;
    assert_eq!(health.expect(StatusCode::OK)["status"], "ok");
    let ready = request(&app.app, "GET", "/readyz", None, None).await;
    assert_eq!(ready.expect(StatusCode::OK)["storage"], "local");
}

#[tokio::test]
async fn share_password_protection() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;
    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=secret.txt"),
        &token,
        "text/plain",
        b"top-secret",
    )
    .await;
    let file_id = uploaded.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let share = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/shares"),
        Some(&token),
        Some(&json!({ "password": "pass1234" })),
    )
    .await;
    let share_token = share.expect(StatusCode::CREATED)["token"]
        .as_str()
        .unwrap()
        .to_string();

    // 无密码 → 401
    let missing = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}"),
        None,
        None,
    )
    .await;
    let missing = missing.expect(StatusCode::UNAUTHORIZED);
    assert_eq!(missing["code"], "DRIVE_SHARE_PASSWORD_REQUIRED");

    // 错误密码 → 401
    let wrong = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}?password=nope"),
        None,
        None,
    )
    .await;
    let wrong = wrong.expect(StatusCode::UNAUTHORIZED);
    assert_eq!(wrong["code"], "DRIVE_SHARE_PASSWORD_INVALID");

    // 正确密码 → 200 且标记受保护
    let info = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}?password=pass1234"),
        None,
        None,
    )
    .await;
    let info = info.expect(StatusCode::OK);
    assert_eq!(info["passwordProtected"], true);

    // 带密码下载成功
    let download = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/public/shares/{share_token}/download?password=pass1234"),
        None,
        None,
    )
    .await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.bytes, b"top-secret");

    // 密码过短 → 422
    let short = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/shares"),
        Some(&token),
        Some(&json!({ "password": "x" })),
    )
    .await;
    short.expect(StatusCode::UNPROCESSABLE_ENTITY);
}
