//! 分片上传会话表。

use sea_orm_migration::prelude::*;

/// 迁移定义。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UploadSessions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UploadSessions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UploadSessions::SpaceId).uuid().not_null())
                    .col(ColumnDef::new(UploadSessions::ParentId).uuid())
                    .col(
                        ColumnDef::new(UploadSessions::Name)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(ColumnDef::new(UploadSessions::Mime).string_len(128))
                    .col(
                        ColumnDef::new(UploadSessions::Size)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UploadSessions::StorageKey)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UploadSessions::ReceivedParts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(UploadSessions::CreatedBy).uuid().not_null())
                    .col(
                        ColumnDef::new(UploadSessions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UploadSessions::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UploadSessions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// upload_sessions 表标识符。
#[derive(DeriveIden)]
enum UploadSessions {
    /// 表。
    Table,
    /// id。
    Id,
    /// space_id。
    SpaceId,
    /// parent_id。
    ParentId,
    /// name。
    Name,
    /// mime。
    Mime,
    /// size。
    Size,
    /// storage_key。
    StorageKey,
    /// received_parts。
    ReceivedParts,
    /// created_by。
    CreatedBy,
    /// created_at。
    CreatedAt,
    /// updated_at。
    UpdatedAt,
}
