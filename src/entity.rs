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

/// 分享链接。
pub mod share {
    use sea_orm::entity::prelude::*;

    /// 只读权限（当前唯一支持）。
    pub const PERMISSION_READ: &str = "read";

    /// 分享模型（token 全局唯一、支持有效期与下载次数上限）。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "shares")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 分享节点。
        pub node_id: Uuid,
        /// 随机 token（URL 安全）。
        pub token: String,
        /// 权限：read。
        pub permission: String,
        /// 过期时间（空 = 不过期）。
        #[sea_orm(nullable)]
        pub expires_at: Option<DateTimeWithTimeZone>,
        /// 最大下载次数（0 = 不限）。
        pub max_downloads: i64,
        /// 已下载次数。
        pub download_count: i64,
        /// 创建人。
        pub created_by: Uuid,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
        /// 吊销时间。
        #[sea_orm(nullable)]
        pub revoked_at: Option<DateTimeWithTimeZone>,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
