//! 数据访问层：只操作 drive schema。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set,
};
use uuid::Uuid;

use club_common::{new_id, AppError};

use crate::entity::{node, space, space_member};

/// 数据库错误 → 统一错误。
pub fn map_db_err(err: DbErr) -> AppError {
    AppError::internal(err)
}

/// 创建空间并授权所有者。
pub async fn create_space(
    db: &DatabaseConnection,
    owner_id: Uuid,
    name: &str,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<space::Model, AppError> {
    let model = space::ActiveModel {
        id: Set(new_id()),
        name: Set(name.to_string()),
        r#type: Set(kind.to_string()),
        owner_id: Set(owner_id),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    space_member::ActiveModel {
        space_id: Set(model.id),
        user_id: Set(owner_id),
        role: Set(space_member::ROLE_ADMIN.to_string()),
        joined_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    Ok(model)
}

/// 列出用户可访问的空间。
pub async fn list_spaces(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<space::Model>, AppError> {
    let members = space_member::Entity::find()
        .filter(space_member::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(map_db_err)?;
    let ids: Vec<Uuid> = members.iter().map(|m| m.space_id).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    space::Entity::find()
        .filter(space::Column::Id.is_in(ids))
        .order_by_asc(space::Column::CreatedAt)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 校验用户是空间成员。
pub async fn ensure_member(
    db: &DatabaseConnection,
    space_id: Uuid,
    user_id: Uuid,
) -> Result<space_member::Model, AppError> {
    space_member::Entity::find_by_id((space_id, user_id))
        .one(db)
        .await
        .map_err(map_db_err)?
        .ok_or_else(|| AppError::forbidden("DRIVE_NOT_MEMBER", "无权访问该空间"))
}

/// 创建文件夹。
pub async fn create_folder(
    db: &DatabaseConnection,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    name: &str,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    node::ActiveModel {
        id: Set(new_id()),
        space_id: Set(space_id),
        parent_id: Set(parent_id),
        name: Set(name.to_string()),
        kind: Set(node::KIND_FOLDER.to_string()),
        storage_key: Set(None),
        size: Set(0),
        mime: Set(None),
        hash: Set(None),
        created_by: Set(user_id),
        deleted_at: Set(None),
        created_at: Set(now.fixed_offset()),
        updated_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 创建文件节点（存储写入成功后调用）。
#[allow(clippy::too_many_arguments)]
pub async fn create_file(
    db: &DatabaseConnection,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    name: &str,
    mime: Option<String>,
    storage_key: String,
    size: i64,
    hash: Option<String>,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    node::ActiveModel {
        id: Set(new_id()),
        space_id: Set(space_id),
        parent_id: Set(parent_id),
        name: Set(name.to_string()),
        kind: Set(node::KIND_FILE.to_string()),
        storage_key: Set(Some(storage_key)),
        size: Set(size),
        mime: Set(mime),
        hash: Set(hash),
        created_by: Set(user_id),
        deleted_at: Set(None),
        created_at: Set(now.fixed_offset()),
        updated_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 列出目录内容（`trashed` 为真时列出回收站）。
pub async fn list_nodes(
    db: &DatabaseConnection,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    trashed: bool,
) -> Result<Vec<node::Model>, AppError> {
    let mut select = node::Entity::find().filter(node::Column::SpaceId.eq(space_id));
    select = if trashed {
        select.filter(node::Column::DeletedAt.is_not_null())
    } else {
        select
            .filter(node::Column::DeletedAt.is_null())
            .filter(match parent_id {
                Some(parent_id) => node::Column::ParentId.eq(parent_id),
                None => node::Column::ParentId.is_null(),
            })
    };
    select
        .order_by_asc(node::Column::Kind)
        .order_by_asc(node::Column::Name)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 按 ID 查找节点（含已删除）。
pub async fn find_node(
    db: &DatabaseConnection,
    node_id: Uuid,
) -> Result<Option<node::Model>, AppError> {
    node::Entity::find_by_id(node_id)
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 软删除节点（进回收站）。
pub async fn soft_delete(
    db: &DatabaseConnection,
    node: &node::Model,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    let mut active: node::ActiveModel = node.clone().into();
    active.deleted_at = Set(Some(now.fixed_offset()));
    active.updated_at = Set(now.fixed_offset());
    active.update(db).await.map_err(map_db_err)
}

/// 从回收站恢复。
pub async fn restore(
    db: &DatabaseConnection,
    node: &node::Model,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    let mut active: node::ActiveModel = node.clone().into();
    active.deleted_at = Set(None);
    active.updated_at = Set(now.fixed_offset());
    active.update(db).await.map_err(map_db_err)
}
