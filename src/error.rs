use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("github error: {0}")]
    Github(#[from] octocrab::Error),
    #[error("github: {0}")]
    GithubMsg(String),
    #[error("discord error: {0}")]
    Discord(#[from] serenity::Error),
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("umf validation failed: {0}")]
    Umf(String),
}

pub type Result<T> = std::result::Result<T, Error>;
