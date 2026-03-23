use thiserror::Error;
use yoink_shared::YoinkError;

use crate::providers::ProviderError;

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("HTTP error during {operation}: {source}")]
    Http {
        operation: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("Filesystem error during {operation} ({path}): {source}")]
    Filesystem {
        operation: String,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Metadata error during {operation}: {reason}")]
    Metadata { operation: String, reason: String },
    #[error("Download pipeline error at {stage}: {reason}")]
    DownloadPipeline { stage: String, reason: String },
    #[error("Background task join error: {reason}")]
    TaskJoin { reason: String },
    #[error("Not found: {resource}")]
    NotFound {
        resource: String,
        id: Option<String>,
    },
    #[error("Validation failed: {reason}")]
    Validation {
        field: Option<String>,
        reason: String,
    },
    #[error("Conflict: {reason}")]
    Conflict { reason: String },
    #[error("Service unavailable: {service}")]
    Unavailable { service: String, reason: String },
}

impl AppError {
    pub(crate) fn not_found(resource: impl Into<String>, id: Option<impl Into<String>>) -> Self {
        Self::NotFound {
            resource: resource.into(),
            id: id.map(Into::into),
        }
    }

    pub(crate) fn validation(field: Option<impl Into<String>>, reason: impl Into<String>) -> Self {
        Self::Validation {
            field: field.map(Into::into),
            reason: reason.into(),
        }
    }

    pub(crate) fn conflict(reason: impl Into<String>) -> Self {
        Self::Conflict {
            reason: reason.into(),
        }
    }

    pub(crate) fn unavailable(service: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            service: service.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn http(operation: impl Into<String>, source: reqwest::Error) -> Self {
        Self::Http {
            operation: operation.into(),
            source,
        }
    }

    pub(crate) fn filesystem(
        operation: impl Into<String>,
        path: impl Into<String>,
        source: std::io::Error,
    ) -> Self {
        Self::Filesystem {
            operation: operation.into(),
            path: path.into(),
            source,
        }
    }

    pub(crate) fn metadata(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Metadata {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn download(stage: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::DownloadPipeline {
            stage: stage.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn task_join(reason: impl Into<String>) -> Self {
        Self::TaskJoin {
            reason: reason.into(),
        }
    }
}

impl From<AppError> for YoinkError {
    fn from(value: AppError) -> Self {
        match value {
            AppError::NotFound { resource, id } => YoinkError::NotFound { resource, id },
            AppError::Validation { field, reason } => YoinkError::Validation { field, reason },
            AppError::Conflict { reason } => YoinkError::Conflict { reason },
            AppError::Unavailable { service, reason } => {
                YoinkError::Unavailable { service, reason }
            }
            AppError::SeaOrm(err) => YoinkError::Internal {
                message: format!("sea-orm error: {err}"),
            },
            AppError::Io(err) => YoinkError::Internal {
                message: format!("io error: {err}"),
            },
            AppError::Uuid(err) => YoinkError::Validation {
                field: None,
                reason: format!("invalid UUID: {err}"),
            },
            AppError::Provider(err) => YoinkError::Unavailable {
                service: "provider".to_string(),
                reason: err.to_string(),
            },
            AppError::Http { operation, source } => YoinkError::Unavailable {
                service: format!("http:{operation}"),
                reason: source.to_string(),
            },
            AppError::Filesystem {
                operation,
                path,
                source,
            } => YoinkError::Internal {
                message: format!("filesystem:{operation}:{path}: {source}"),
            },
            AppError::Metadata { operation, reason } => YoinkError::Internal {
                message: format!("metadata:{operation}: {reason}"),
            },
            AppError::DownloadPipeline { stage, reason } => YoinkError::Internal {
                message: format!("download:{stage}: {reason}"),
            },
            AppError::TaskJoin { reason } => YoinkError::Internal {
                message: format!("task_join: {reason}"),
            },
        }
    }
}
