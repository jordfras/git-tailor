// Core library for git-scissors

pub mod repo;

/// Represents commit metadata extracted from git repository.
///
/// This is a pure data structure containing commit information
/// without any git2 object dependencies.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub date: String,
    pub parent_oids: Vec<String>,
}
