//! shares 增加分享密码列。

use sea_orm_migration::prelude::*;

/// 迁移定义。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Shares::Table)
                    .add_column_if_not_exists(ColumnDef::new(Shares::PasswordHash).string_len(255))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Shares::Table)
                    .drop_column(Shares::PasswordHash)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// shares 表标识符（与首个迁移列名保持一致）。
#[derive(DeriveIden)]
enum Shares {
    /// 表。
    Table,
    /// password_hash。
    PasswordHash,
}
