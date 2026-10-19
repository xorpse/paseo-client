use serde::{Deserialize, Serialize};

use crate::error::ProtocolViolation;
use crate::protocol::number::WireNumber;
use crate::protocol::workspaces::AheadBehind;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    Base,
    Uncommitted,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCompare {
    mode: DiffMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_whitespace: Option<bool>,
}

impl DiffCompare {
    pub fn uncommitted() -> DiffCompare {
        DiffCompare {
            mode: DiffMode::Uncommitted,
            base_ref: None,
            ignore_whitespace: None,
        }
    }

    pub fn base(base_ref: Option<String>) -> DiffCompare {
        DiffCompare {
            mode: DiffMode::Base,
            base_ref,
            ignore_whitespace: None,
        }
    }

    pub fn mode(&self) -> DiffMode {
        self.mode
    }

    pub fn base_ref(&self) -> Option<&str> {
        self.base_ref.as_deref()
    }

    pub fn ignore_whitespace(&self) -> Option<bool> {
        self.ignore_whitespace
    }

    pub fn set_ignore_whitespace(&mut self, ignore_whitespace: bool) {
        self.ignore_whitespace = Some(ignore_whitespace);
    }

    pub fn with_ignore_whitespace(mut self, ignore_whitespace: bool) -> DiffCompare {
        self.set_ignore_whitespace(ignore_whitespace);
        self
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Add,
    Context,
    Header,
    Remove,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DiffLine {
    #[serde(rename = "type")]
    kind: DiffLineKind,
    content: String,
}

impl DiffLine {
    pub fn kind(&self) -> DiffLineKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    old_start: WireNumber,
    old_count: WireNumber,
    new_start: WireNumber,
    new_count: WireNumber,
    lines: Vec<DiffLine>,
}

impl DiffHunk {
    pub fn old_start(&self) -> u64 {
        self.old_start.count()
    }

    pub fn old_count(&self) -> u64 {
        self.old_count.count()
    }

    pub fn new_start(&self) -> u64 {
        self.new_start.count()
    }

    pub fn new_count(&self) -> u64 {
        self.new_count.count()
    }

    pub fn lines(&self) -> &[DiffLine] {
        &self.lines
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffFileStatus {
    Binary,
    Ok,
    TooLarge,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    path: String,
    #[serde(default)]
    old_path: Option<String>,
    is_new: bool,
    is_deleted: bool,
    additions: WireNumber,
    deletions: WireNumber,
    hunks: Vec<DiffHunk>,
    #[serde(default)]
    status: Option<DiffFileStatus>,
}

impl DiffFile {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn old_path(&self) -> Option<&str> {
        self.old_path.as_deref()
    }

    pub fn is_new(&self) -> bool {
        self.is_new
    }

    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }

    pub fn additions(&self) -> u64 {
        self.additions.count()
    }

    pub fn deletions(&self) -> u64 {
        self.deletions.count()
    }

    pub fn hunks(&self) -> &[DiffHunk] {
        &self.hunks
    }

    pub fn status(&self) -> Option<DiffFileStatus> {
        self.status
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckoutErrorCode {
    MergeConflict,
    NotAllowed,
    NotGitRepo,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CheckoutError {
    code: CheckoutErrorCode,
    message: String,
}

impl CheckoutError {
    pub fn code(&self) -> CheckoutErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDiff {
    cwd: String,
    files: Vec<DiffFile>,
    error: Option<CheckoutError>,
    #[serde(default)]
    diff_too_large: bool,
}

impl CheckoutDiff {
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn files(&self) -> &[DiffFile] {
        &self.files
    }

    pub fn error(&self) -> Option<&CheckoutError> {
        self.error.as_ref()
    }

    pub fn diff_too_large(&self) -> bool {
        self.diff_too_large
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(try_from = "CheckoutStatusWire")]
pub struct CheckoutStatus {
    cwd: String,
    error: Option<CheckoutError>,
    has_remote: bool,
    upstream_ref: Option<String>,
    repository: Option<GitRepositoryStatus>,
}

impl CheckoutStatus {
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn error(&self) -> Option<&CheckoutError> {
        self.error.as_ref()
    }

    pub fn has_remote(&self) -> bool {
        self.has_remote
    }

    pub fn upstream_ref(&self) -> Option<&str> {
        self.upstream_ref.as_deref()
    }

    pub fn repository(&self) -> Option<&GitRepositoryStatus> {
        self.repository.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GitRepositoryStatus {
    repo_root: String,
    main_repo_root: Option<String>,
    is_paseo_owned_worktree: bool,
    current_branch: Option<String>,
    is_dirty: bool,
    base_ref: Option<String>,
    ahead_behind: Option<AheadBehind>,
    ahead_of_origin: Option<WireNumber>,
    behind_of_origin: Option<WireNumber>,
    remote_url: Option<String>,
}

impl GitRepositoryStatus {
    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }

    pub fn main_repo_root(&self) -> Option<&str> {
        self.main_repo_root.as_deref()
    }

    pub fn is_paseo_owned_worktree(&self) -> bool {
        self.is_paseo_owned_worktree
    }

    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn base_ref(&self) -> Option<&str> {
        self.base_ref.as_deref()
    }

    pub fn ahead_behind(&self) -> Option<AheadBehind> {
        self.ahead_behind
    }

    pub fn ahead_of_origin(&self) -> Option<u64> {
        self.ahead_of_origin.map(WireNumber::count)
    }

    pub fn behind_of_origin(&self) -> Option<u64> {
        self.behind_of_origin.map(WireNumber::count)
    }

    pub fn remote_url(&self) -> Option<&str> {
        self.remote_url.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutStatusWire {
    cwd: String,
    error: Option<CheckoutError>,
    #[serde(default)]
    upstream_ref: Option<String>,
    is_git: bool,
    #[serde(default)]
    is_paseo_owned_worktree: bool,
    #[serde(default)]
    repo_root: Option<String>,
    #[serde(default)]
    main_repo_root: Option<String>,
    #[serde(default)]
    current_branch: Option<String>,
    #[serde(default)]
    is_dirty: Option<bool>,
    #[serde(default)]
    base_ref: Option<String>,
    #[serde(default)]
    ahead_behind: Option<AheadBehind>,
    #[serde(default)]
    ahead_of_origin: Option<WireNumber>,
    #[serde(default)]
    behind_of_origin: Option<WireNumber>,
    has_remote: bool,
    #[serde(default)]
    remote_url: Option<String>,
}

impl TryFrom<CheckoutStatusWire> for CheckoutStatus {
    type Error = ProtocolViolation;

    fn try_from(wire: CheckoutStatusWire) -> Result<CheckoutStatus, ProtocolViolation> {
        let repository = if wire.is_git {
            Some(GitRepositoryStatus {
                repo_root: wire
                    .repo_root
                    .ok_or(ProtocolViolation::IncompleteCheckoutStatus("repoRoot"))?,
                main_repo_root: wire.main_repo_root,
                is_paseo_owned_worktree: wire.is_paseo_owned_worktree,
                current_branch: wire.current_branch,
                is_dirty: wire
                    .is_dirty
                    .ok_or(ProtocolViolation::IncompleteCheckoutStatus("isDirty"))?,
                base_ref: wire.base_ref,
                ahead_behind: wire.ahead_behind,
                ahead_of_origin: wire.ahead_of_origin,
                behind_of_origin: wire.behind_of_origin,
                remote_url: wire.remote_url,
            })
        } else {
            None
        };
        Ok(CheckoutStatus {
            cwd: wire.cwd,
            error: wire.error,
            has_remote: wire.has_remote,
            upstream_ref: wire.upstream_ref,
            repository,
        })
    }
}
