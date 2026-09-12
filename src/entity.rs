//! drive schema 实体。

/// 空间。
pub mod space {
    use sea_orm::entity::prelude::*;

    /// 个人空间。
    pub const TYPE_PERSONAL: &str = "personal";
    /// 公共空间。
    pub const TYPE_PUBLIC: &str = "public";
    /// 团队空间。
    pub const TYPE_TEAM: &str = "team";

    /// 空间模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "spaces")]
    pub struct Model {
        /// 空间 ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 名称。
        pub name: String,
        /// personal / public / team。
        pub r#type: String,
        /// 所有者。
        pub owner_id: Uuid,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 空间成员。
pub mod space_member {
    use sea_orm::entity::prelude::*;

    /// 管理人。
    pub const ROLE_ADMIN: &str = "admin";
    /// 编辑。
    pub const ROLE_EDITOR: &str = "editor";
    /// 只读。
    pub const ROLE_VIEWER: &str = "viewer";

    /// 成员模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "space_members")]
    pub struct Model {
        /// 空间。
        #[sea_orm(primary_key, auto_increment = false)]
        pub space_id: Uuid,
        /// 用户。
        #[sea_orm(primary_key, auto_increment = false)]
        pub user_id: Uuid,
        /// 角色。
        pub role: String,
        /// 加入时间。
        pub joined_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 文件/文件夹节点。
pub mod node {
    use sea_orm::entity::prelude::*;

    /// 文件夹。
    pub const KIND_FOLDER: &str = "folder";
    /// 文件。
    pub const KIND_FILE: &str = "file";

    /// 节点模型（软删除进回收站）。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "nodes")]
    pub struct Model {
        /// 节点 ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 所属空间。
        pub space_id: Uuid,
        /// 父节点（根为空）。
        #[sea_orm(nullable)]
        pub parent_id: Option<Uuid>,
        /// 文件/文件夹名。
        pub name: String,
        /// folder / file。
        pub kind: String,
        /// 存储 Key（文件夹为空）。
        #[sea_orm(nullable)]
        pub storage_key: Option<String>,
        /// 字节大小。
        pub size: i64,
        /// MIME。
        #[sea_orm(nullable)]
        pub mime: Option<String>,
        /// SHA-256。
        #[sea_orm(nullable)]
        pub hash: Option<String>,
        /// 创建者。
        pub created_by: Uuid,
        /// 回收站时间（NULL 表示正常）。
        #[sea_orm(nullable)]
        pub deleted_at: Option<DateTimeWithTimeZone>,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
        /// 更新时间。
        pub updated_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Clone, Copy, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
