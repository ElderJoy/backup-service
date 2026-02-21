use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// -- Newtype wrappers to prevent argument mix-ups --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
#[allow(dead_code)]
pub struct TenantId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
#[allow(dead_code)]
pub struct BackupId(pub Uuid);

// -- Backup status enum --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "running")]
    Running,
    #[sqlx(rename = "completed")]
    Completed,
    #[sqlx(rename = "failed")]
    Failed,
}

impl std::fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

// -- Database row model --

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Backup {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source_path: String,
    pub size_bytes: i64,
    pub status: String,
    pub encryption_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// -- API request/response types --

#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    pub source_path: String,
    #[serde(default)]
    pub encryption_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBackupRequest {
    pub status: Option<BackupStatus>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source_path: String,
    pub size_bytes: i64,
    pub status: String,
    pub encryption_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entropy: Option<f64>,
}

impl From<Backup> for BackupResponse {
    fn from(b: Backup) -> Self {
        Self {
            id: b.id,
            tenant_id: b.tenant_id,
            source_path: b.source_path,
            size_bytes: b.size_bytes,
            status: b.status,
            encryption_enabled: b.encryption_enabled,
            created_at: b.created_at,
            updated_at: b.updated_at,
            entropy: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

// -- JWT Claims --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub tenant_id: Uuid,
    pub roles: Vec<String>,
    pub exp: usize,
    pub iat: usize,
}
