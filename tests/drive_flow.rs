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

#[tokio::test]
async fn multipart_upload_flow() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    let created = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/uploads"),
        Some(&token),
        Some(&json!({ "name": "big.bin", "size": 11, "mime": "application/octet-stream" })),
    )
    .await;
    let session = created.expect(StatusCode::CREATED);
    let upload_id = session["id"].as_str().unwrap().to_string();

    // 分片必须严格按序
    let out_of_order = upload_put(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/parts/2"),
        &token,
        "application/octet-stream",
        b"world",
    )
    .await;
    out_of_order.expect(StatusCode::CONFLICT);

    let part1 = upload_put(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/parts/1"),
        &token,
        "application/octet-stream",
        b"hello ",
    )
    .await;
    assert_eq!(part1.expect(StatusCode::OK)["received"], 1);
    let part2 = upload_put(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/parts/2"),
        &token,
        "application/octet-stream",
        b"world",
    )
    .await;
    assert_eq!(part2.expect(StatusCode::OK)["received"], 2);

    let completed = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/complete"),
        Some(&token),
        None,
    )
    .await;
    let completed = completed.expect(StatusCode::CREATED);
    let file_id = completed["id"].as_str().unwrap().to_string();

    // 合并后的内容一致
    let download = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/download"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(download.bytes, b"hello world");
}

#[tokio::test]
async fn presign_fallback_and_upload_cancel() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    // 本地后端不支持预签名直传
    let presign = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/files/presign"),
        Some(&token),
        Some(&json!({ "name": "a.txt" })),
    )
    .await;
    let presign = presign.expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(presign["code"], "DRIVE_PRESIGN_UNSUPPORTED");

    // 直传完成校验 storageKey 前缀
    let bad_complete = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/files/complete"),
        Some(&token),
        Some(&json!({ "storageKey": "drive/other-space/key", "name": "a.txt", "size": 1 })),
    )
    .await;
    bad_complete.expect(StatusCode::FORBIDDEN);

    // 空分片完成 → 422；取消后会话消失
    let created = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/uploads"),
        Some(&token),
        Some(&json!({ "name": "cancel.bin", "size": 10 })),
    )
    .await;
    let upload_id = created.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let empty_complete = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/complete"),
        Some(&token),
        None,
    )
    .await;
    empty_complete.expect(StatusCode::UNPROCESSABLE_ENTITY);

    let cancel = request(
        &app.app,
        "DELETE",
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}"),
        Some(&token),
        None,
    )
    .await;
    cancel.expect(StatusCode::NO_CONTENT);

    let after = upload_put(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/uploads/{upload_id}/parts/1"),
        &token,
        "application/octet-stream",
        b"x",
    )
    .await;
    after.expect(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn office_preview_config_and_wopi_endpoints() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    // 上传 Word 文件
    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=report.docx&mime=application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        &token,
        "application/octet-stream",
        b"docx-bytes",
    )
    .await;
    let file_id = uploaded.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // OnlyOffice 配置
    let config = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/office-config"),
        Some(&token),
        None,
    )
    .await;
    let config = config.expect(StatusCode::OK);
    assert_eq!(config["documentServerUrl"], "http://office.test");
    assert_eq!(config["config"]["documentType"], "word");
    assert!(config["config"]["document"]["url"]
        .as_str()
        .unwrap()
        .starts_with("http://drive.test/wopi/files/"));
    assert!(!config["token"].as_str().unwrap().is_empty());

    // 不支持的扩展名 → 422
    let image = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=photo.png"),
        &token,
        "image/png",
        b"png",
    )
    .await;
    let image_id = image.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let unsupported = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{image_id}/office-config"),
        Some(&token),
        None,
    )
    .await;
    unsupported.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // WOPI 令牌 + CheckFileInfo + GetFile
    let wopi_token_response = request(
        &app.app,
        "POST",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/office-token"),
        Some(&token),
        None,
    )
    .await;
    let wopi = wopi_token_response.expect(StatusCode::OK);
    let access_token = wopi["token"].as_str().unwrap().to_string();
    let expires = wopi["expires"].as_i64().unwrap();
    let query = format!("access_token={access_token}&expires={expires}");

    let info = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/wopi/files/{file_id}?{query}"),
        None,
        None,
    )
    .await;
    let info = info.expect(StatusCode::OK);
    assert_eq!(info["BaseFileName"], "report.docx");
    assert_eq!(info["ReadOnly"], true);

    let contents = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/wopi/files/{file_id}/contents?{query}"),
        None,
        None,
    )
    .await;
    assert_eq!(contents.status, StatusCode::OK);
    assert_eq!(contents.bytes, b"docx-bytes");

    // 伪造/缺失令牌 → 403
    let forged = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/wopi/files/{file_id}?access_token=forged&expires={expires}"),
        None,
        None,
    )
    .await;
    forged.expect(StatusCode::FORBIDDEN);
    let missing = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/wopi/files/{file_id}"),
        None,
        None,
    )
    .await;
    missing.expect(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn office_config_requires_configured_secret() {
    let app = spawn_with_extra(&[("ONLYOFFICE_JWT_SECRET", "")]).await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;
    let uploaded = upload(
        &app.app,
        &format!("/api/v1/drive/spaces/{space_id}/files?name=a.docx"),
        &token,
        "application/octet-stream",
        b"x",
    )
    .await;
    let file_id = uploaded.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = request(
        &app.app,
        "GET",
        &format!("/api/v1/drive/spaces/{space_id}/nodes/{file_id}/office-config"),
        Some(&token),
        None,
    )
    .await;
    response.expect(StatusCode::SERVICE_UNAVAILABLE);
}
