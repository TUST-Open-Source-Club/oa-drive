//! drive schema 迁移。

use sea_orm_migration::prelude::*;

/// 初始化迁移。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS drive")
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Spaces::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Spaces::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Spaces::Name).string_len(64).not_null())
                    .col(ColumnDef::new(Spaces::Type).string_len(16).not_null())
                    .col(ColumnDef::new(Spaces::OwnerId).uuid().not_null())
                    .col(
                        ColumnDef::new(Spaces::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SpaceMembers::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SpaceMembers::SpaceId).uuid().not_null())
                    .col(ColumnDef::new(SpaceMembers::UserId).uuid().not_null())
                    .col(ColumnDef::new(SpaceMembers::Role).string_len(16).not_null())
                    .col(
                        ColumnDef::new(SpaceMembers::JoinedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(SpaceMembers::SpaceId)
                            .col(SpaceMembers::UserId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Nodes::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Nodes::SpaceId).uuid().not_null())
                    .col(ColumnDef::new(Nodes::ParentId).uuid())
                    .col(ColumnDef::new(Nodes::Name).string_len(255).not_null())
                    .col(ColumnDef::new(Nodes::Kind).string_len(16).not_null())
                    .col(ColumnDef::new(Nodes::StorageKey).string_len(512))
                    .col(
                        ColumnDef::new(Nodes::Size)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Nodes::Mime).string_len(128))
                    .col(ColumnDef::new(Nodes::Hash).string_len(64))
                    .col(ColumnDef::new(Nodes::CreatedBy).uuid().not_null())
                    .col(ColumnDef::new(Nodes::DeletedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Nodes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Nodes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Shares::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Shares::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Shares::NodeId).uuid().not_null())
                    .col(ColumnDef::new(Shares::Token).string_len(128).not_null())
                    .col(ColumnDef::new(Shares::Permission).string_len(16).not_null())
                    .col(ColumnDef::new(Shares::ExpiresAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Shares::MaxDownloads)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Shares::DownloadCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Shares::CreatedBy).uuid().not_null())
                    .col(
                        ColumnDef::new(Shares::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Shares::RevokedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_shares_token")
                    .table(Shares::Table)
                    .col(Shares::Token)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_nodes_space_parent")
                    .table(Nodes::Table)
                    .col(Nodes::SpaceId)
                    .col(Nodes::ParentId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Shares::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Nodes::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(SpaceMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Spaces::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}

/// 迁移入口。
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(Migration), Box::new(crate::migration2::Migration)]
    }
}

/// spaces 表标识符。
#[derive(DeriveIden)]
pub enum Spaces {
    /// 表。
    Table,
    /// id。
    Id,
    /// name。
    Name,
    /// type。
    Type,
    /// owner_id。
    OwnerId,
    /// created_at。
    CreatedAt,
}

/// space_members 表标识符。
#[derive(DeriveIden)]
pub enum SpaceMembers {
    /// 表。
    Table,
    /// space_id。
    SpaceId,
    /// user_id。
    UserId,
    /// role。
    Role,
    /// joined_at。
    JoinedAt,
}

/// shares 表标识符。
#[derive(DeriveIden)]
pub enum Shares {
    /// 表。
    Table,
    /// id。
    Id,
    /// node_id。
    NodeId,
    /// token。
    Token,
    /// permission。
    Permission,
    /// expires_at。
    ExpiresAt,
    /// max_downloads。
    MaxDownloads,
    /// download_count。
    DownloadCount,
    /// created_by。
    CreatedBy,
    /// created_at。
    CreatedAt,
    /// revoked_at。
    RevokedAt,
}

/// nodes 表标识符。
#[derive(DeriveIden)]
pub enum Nodes {
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
    /// kind。
    Kind,
    /// storage_key。
    StorageKey,
    /// size。
    Size,
    /// mime。
    Mime,
    /// hash。
    Hash,
    /// created_by。
    CreatedBy,
    /// deleted_at。
    DeletedAt,
    /// created_at。
    CreatedAt,
    /// updated_at。
    UpdatedAt,
}
