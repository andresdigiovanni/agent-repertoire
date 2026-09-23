pub mod agentsetup;
pub mod cli;
pub mod mcp;
pub mod models;
pub mod prompt;
pub mod registry;
pub mod runner;
pub mod search;
pub mod skill;
pub mod storage;
pub mod validation;

#[cfg(test)]
pub(crate) mod test_support;

#[derive(Debug, thiserror::Error)]
pub enum RepError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid tool definition: {0}")]
    InvalidTool(#[from] InvalidToolIssues),
    #[error("invalid arguments: {0}")]
    InvalidArguments(#[from] InvalidArgumentIssues),
    #[error("tool state drifted from catalog: {0}")]
    Drift(String),
    #[error(transparent)]
    Storage(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yml::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to start process: {0}")]
    Spawn(String),
}

#[derive(Debug, thiserror::Error)]
#[error("{}", .0.join("; "))]
pub struct InvalidToolIssues(pub Vec<String>);

#[derive(Debug, thiserror::Error)]
#[error("{}", .0.join("; "))]
pub struct InvalidArgumentIssues(pub Vec<String>);

pub type Result<T> = std::result::Result<T, RepError>;
