//! HTTP 路由。

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{new_id, AppError, FieldError};

use crate::entity::{node, space};
use crate::repo;
use crate::state::SharedState;

/// 存活检查。
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// 就绪检查。
pub async fn readyz(State(state): State<SharedState>) -> Json<Value> {
    match state.db.ping().await {
        Ok(_) => {
            Json(json!({ "status": "ready", "database": "ok", "storage": state.storage.driver() }))
        }
        Err(err) => {
            tracing::error!(error = %err, "数据库就绪检查失败");
            Json(json!({ "status": "degraded", "database": "error" }))
        }
    }
}

/// 解析用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// 节点 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    /// ID。
    pub id: String,
    /// 父节点。
    pub parent_id: Option<String>,
    /// 名称。
    pub name: String,
    /// folder / file。
    pub kind: String,
    /// 大小。
    pub size: i64,
    /// MIME。
    pub mime: Option<String>,
    /// 删除时间。
    pub deleted_at: Option<DateTime<FixedOffset>>,
    /// 创建时间。
    pub created_at: DateTime<FixedOffset>,
}

impl From<&node::Model> for NodeDto {
    fn from(model: &node::Model) -> Self {
        Self {
            id: model.id.to_string(),
            parent_id: model.parent_id.map(|id| id.to_string()),
            name: model.name.clone(),
            kind: model.kind.clone(),
            size: model.size,
            mime: model.mime.clone(),
            deleted_at: model.deleted_at,
            created_at: model.created_at,
        }
    }
}

/// 创建空间请求。
#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    /// 名称。
    pub name: String,
    /// personal / public / team。
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// `POST /spaces`：创建空间。
pub async fn create_space(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(input): Json<CreateSpaceRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let user_id = user_id_of(&auth)?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::unprocessable(
            "DRIVE_VALIDATION",
            "空间名称需为 1 ~ 64 字符",
            vec![FieldError::new("name", "非法")],
        ));
    }
    let kind = input.kind.unwrap_or_else(|| space::TYPE_TEAM.to_string());
    if ![space::TYPE_PERSONAL, space::TYPE_PUBLIC, space::TYPE_TEAM].contains(&kind.as_str()) {
        return Err(AppError::unprocessable(
            "DRIVE_VALIDATION",
            "空间类型不合法",
            vec![FieldError::new("type", "仅支持 personal/public/team")],
        ));
    }
    let model = repo::create_space(&state.db, user_id, name, &kind, state.now()).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": model.id, "name": model.name, "type": model.r#type })),
    ))
}

/// `GET /spaces`：我的空间。
pub async fn list_spaces(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let spaces = repo::list_spaces(&state.db, user_id).await?;
    Ok(Json(json!(spaces
        .iter()
        .map(|space| json!({ "id": space.id, "name": space.name, "type": space.r#type }))
        .collect::<Vec<_>>())))
}

/// 目录查询。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeQuery {
    /// 父目录（空 = 根）。
    pub parent_id: Option<Uuid>,
    /// 是否回收站。
    pub trashed: Option<bool>,
}

/// `GET /spaces/{id}/nodes`：目录内容/回收站。
pub async fn list_nodes(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(space_id): Path<Uuid>,
    Query(query): Query<NodeQuery>,
) -> Result<Json<Vec<NodeDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let items = repo::list_nodes(
        &state.db,
        space_id,
        query.parent_id,
        query.trashed.unwrap_or(false),
    )
    .await?;
    Ok(Json(items.iter().map(NodeDto::from).collect()))
}

/// 创建文件夹请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderRequest {
    /// 名称。
    pub name: String,
    /// 父目录。
    pub parent_id: Option<Uuid>,
}

/// 校验父节点属于同一空间且为文件夹。
async fn validate_parent(
    state: &SharedState,
    space_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), AppError> {
    if let Some(parent_id) = parent_id {
        let parent = repo::find_node(&state.db, parent_id)
            .await?
            .ok_or_else(|| AppError::not_found("DRIVE_NODE_NOT_FOUND", "父目录不存在"))?;
        if parent.space_id != space_id
            || parent.kind != node::KIND_FOLDER
            || parent.deleted_at.is_some()
        {
            return Err(AppError::unprocessable(
                "DRIVE_VALIDATION",
                "父目录不合法",
                vec![FieldError::new("parentId", "必须是同空间的文件夹")],
            ));
        }
    }
    Ok(())
}

/// `POST /spaces/{id}/folders`：新建文件夹。
pub async fn create_folder(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(space_id): Path<Uuid>,
    Json(input): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<NodeDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 255 || name.contains('/') {
        return Err(AppError::unprocessable(
            "DRIVE_VALIDATION",
            "文件夹名称不合法",
            vec![FieldError::new("name", "不能为空或包含 /")],
        ));
    }
    validate_parent(&state, space_id, input.parent_id).await?;
    let model = repo::create_folder(
        &state.db,
        space_id,
        input.parent_id,
        name,
        user_id,
        state.now(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(NodeDto::from(&model))))
}

/// 上传查询参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadQuery {
    /// 文件名。
    pub name: String,
    /// MIME。
    pub mime: Option<String>,
    /// 父目录。
    pub parent_id: Option<Uuid>,
}

/// `POST /spaces/{id}/files`：上传文件（当前为服务端中转；预签名直传后续接入）。
pub async fn upload_file(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(space_id): Path<Uuid>,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<NodeDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let name = query.name.trim();
    if name.is_empty() || name.chars().count() > 255 || name.contains('/') {
        return Err(AppError::unprocessable(
            "DRIVE_VALIDATION",
            "文件名不合法",
            vec![FieldError::new("name", "不能为空或包含 /")],
        ));
    }
    validate_parent(&state, space_id, query.parent_id).await?;

    // Key 不含用户文件名，使用 UUID 避免泄漏与路径穿越
    let storage_key = format!("drive/{}/{}", space_id.simple(), new_id().simple());
    state
        .storage
        .put(
            &storage_key,
            body.clone(),
            query.mime.as_deref().unwrap_or("application/octet-stream"),
        )
        .await
        .map_err(AppError::internal)?;
    let hash = hex::encode(Sha256::digest(&body));
    let model = repo::create_file(
        &state.db,
        space_id,
        query.parent_id,
        name,
        query.mime,
        storage_key,
        body.len() as i64,
        Some(hash),
        user_id,
        state.now(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(NodeDto::from(&model))))
}

/// 加载节点并校验空间权限。
async fn load_node_for_member(
    state: &SharedState,
    user_id: Uuid,
    space_id: Uuid,
    node_id: Uuid,
) -> Result<node::Model, AppError> {
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let model = repo::find_node(&state.db, node_id)
        .await?
        .ok_or_else(|| AppError::not_found("DRIVE_NODE_NOT_FOUND", "节点不存在"))?;
    if model.space_id != space_id {
        return Err(AppError::not_found("DRIVE_NODE_NOT_FOUND", "节点不存在"));
    }
    Ok(model)
}

/// `GET /spaces/{id}/nodes/{node_id}/download`：下载（本地后端流式回源）。
pub async fn download_node(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_node_for_member(&state, user_id, space_id, node_id).await?;
    if model.kind != node::KIND_FILE || model.deleted_at.is_some() {
        return Err(AppError::not_found("DRIVE_NODE_NOT_FOUND", "文件不存在"));
    }
    let key = model
        .storage_key
        .clone()
        .ok_or_else(|| AppError::internal("文件缺少存储 Key"))?;
    let bytes = state.storage.get(&key).await.map_err(AppError::internal)?;
    let content_type = model
        .mime
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    // RFC 5987 文件名编码，防头注入
    let filename = percent_encode_header(&model.name);
    let mut response = bytes.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{}", filename))
    {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// `GET /spaces/{id}/nodes/{node_id}/ticket`：获取带签名的临时下载地址。
pub async fn node_ticket(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_node_for_member(&state, user_id, space_id, node_id).await?;
    let Some(local) = &state.local else {
        return Err(AppError::internal("S3 预签名待接入"));
    };
    let key = model
        .storage_key
        .clone()
        .ok_or_else(|| AppError::internal("文件缺少存储 Key"))?;
    let expires_at = state.now() + Duration::minutes(5);
    Ok(Json(json!({
        "url": format!("/api/v1/drive/spaces/{space_id}/nodes/{node_id}/download?expires={}&signature={}",
            expires_at.timestamp(), local.sign_download(&key, expires_at)),
        "expiresAt": expires_at
    })))
}

/// `DELETE /spaces/{id}/nodes/{node_id}`：移入回收站。
pub async fn delete_node(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_node_for_member(&state, user_id, space_id, node_id).await?;
    repo::soft_delete(&state.db, &model, state.now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /spaces/{id}/nodes/{node_id}/restore`：从回收站恢复。
pub async fn restore_node(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<NodeDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_node_for_member(&state, user_id, space_id, node_id).await?;
    let restored = repo::restore(&state.db, &model, state.now()).await?;
    Ok(Json(NodeDto::from(&restored)))
}

/// 请求头文件名百分号编码。
fn percent_encode_header(name: &str) -> String {
    name.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// `/api/v1/drive` 路由。
pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/spaces", get(list_spaces).post(create_space))
        .route("/spaces/{id}/nodes", get(list_nodes))
        .route("/spaces/{id}/folders", post(create_folder))
        .route("/spaces/{id}/files", post(upload_file))
        .route("/spaces/{id}/nodes/{node_id}", delete(delete_node))
        .route("/spaces/{id}/nodes/{node_id}/restore", post(restore_node))
        .route("/spaces/{id}/nodes/{node_id}/download", get(download_node))
        .route("/spaces/{id}/nodes/{node_id}/ticket", get(node_ticket))
}
