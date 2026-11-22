use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentReview {
    agent_id: String,
    repositories: Vec<ReviewRepository>,
    turns: Vec<ReviewTurn>,
    seen: Vec<SeenFile>,
    comments: Vec<ReviewComment>,
}

impl AgentReview {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn repositories(&self) -> &[ReviewRepository] {
        &self.repositories
    }

    pub fn turns(&self) -> &[ReviewTurn] {
        &self.turns
    }

    pub fn seen(&self) -> &[SeenFile] {
        &self.seen
    }

    pub fn comments(&self) -> &[ReviewComment] {
        &self.comments
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRepository {
    root: String,
    base_ref: Option<String>,
    belongs_to: Option<String>,
    files: Vec<ReviewFile>,
}

impl ReviewRepository {
    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn base_ref(&self) -> Option<&str> {
        self.base_ref.as_deref()
    }

    pub fn belongs_to(&self) -> Option<&str> {
        self.belongs_to.as_deref()
    }

    pub fn files(&self) -> &[ReviewFile] {
        &self.files
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    path: String,
    tool_calls: Vec<ToolCallReference>,
}

impl ReviewFile {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn tool_calls(&self) -> &[ToolCallReference] {
        &self.tool_calls
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallReference {
    call_id: String,
    subagent_id: Option<String>,
    turn_index: Option<u32>,
}

impl ToolCallReference {
    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    pub fn subagent_id(&self) -> Option<&str> {
        self.subagent_id.as_deref()
    }

    pub fn turn_index(&self) -> Option<u32> {
        self.turn_index
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnOrigin {
    Autonomous,
    Prompt,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTurn {
    index: u32,
    origin: TurnOrigin,
    message_id: Option<String>,
    turn_id: Option<String>,
    started_at: String,
    heads: Vec<TurnHead>,
}

impl ReviewTurn {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn origin(&self) -> TurnOrigin {
        self.origin
    }

    pub fn message_id(&self) -> Option<&str> {
        self.message_id.as_deref()
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn started_at(&self) -> &str {
        &self.started_at
    }

    pub fn heads(&self) -> &[TurnHead] {
        &self.heads
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnHead {
    repo_root: String,
    commit: String,
}

impl TurnHead {
    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }

    pub fn commit(&self) -> &str {
        &self.commit
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeenFile {
    repo_root: String,
    path: String,
    fingerprint: Option<String>,
}

impl SeenFile {
    pub fn new(repo_root: String, path: String, fingerprint: Option<String>) -> SeenFile {
        SeenFile {
            repo_root,
            path,
            fingerprint,
        }
    }

    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReference {
    repo_root: String,
    path: String,
}

impl FileReference {
    pub fn new(repo_root: String, path: String) -> FileReference {
        FileReference { repo_root, path }
    }

    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CommentState {
    Draft,
    #[serde(rename_all = "camelCase")]
    Resolved {
        message_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Sent {
        message_id: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentSide {
    #[default]
    New,
    Old,
    #[serde(other)]
    Unknown,
}

impl CommentSide {
    fn is_new(&self) -> bool {
        *self == CommentSide::New
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommentContext {
    before: String,
    target: String,
    after: String,
}

impl CommentContext {
    pub fn new(before: String, target: String, after: String) -> CommentContext {
        CommentContext {
            before,
            target,
            after,
        }
    }

    pub fn before(&self) -> &str {
        &self.before
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn after(&self) -> &str {
        &self.after
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentInput {
    id: String,
    parent_id: Option<String>,
    repo_root: String,
    path: String,
    start_line: u32,
    end_line: u32,
    #[serde(skip_serializing_if = "CommentSide::is_new")]
    side: CommentSide,
    context: CommentContext,
    fingerprint: Option<String>,
    body: String,
    state: CommentState,
}

impl ReviewCommentInput {
    pub fn new(
        id: String,
        location: FileReference,
        lines: RangeInclusive<u32>,
        context: CommentContext,
        body: String,
    ) -> ReviewCommentInput {
        ReviewCommentInput {
            id,
            parent_id: None,
            repo_root: location.repo_root,
            path: location.path,
            start_line: *lines.start(),
            end_line: *lines.end(),
            side: CommentSide::New,
            context,
            fingerprint: None,
            body,
            state: CommentState::Draft,
        }
    }

    pub fn set_parent_id(&mut self, parent_id: String) {
        self.parent_id = Some(parent_id);
    }

    pub fn with_parent_id(mut self, parent_id: String) -> ReviewCommentInput {
        self.set_parent_id(parent_id);
        self
    }

    pub fn side(&self) -> CommentSide {
        self.side
    }

    pub fn set_side(&mut self, side: CommentSide) {
        self.side = side;
    }

    pub fn with_side(mut self, side: CommentSide) -> ReviewCommentInput {
        self.set_side(side);
        self
    }

    pub fn set_fingerprint(&mut self, fingerprint: String) {
        self.fingerprint = Some(fingerprint);
    }

    pub fn with_fingerprint(mut self, fingerprint: String) -> ReviewCommentInput {
        self.set_fingerprint(fingerprint);
        self
    }

    pub fn set_state(&mut self, state: CommentState) {
        self.state = state;
    }

    pub fn with_state(mut self, state: CommentState) -> ReviewCommentInput {
        self.set_state(state);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    id: String,
    parent_id: Option<String>,
    repo_root: String,
    path: String,
    start_line: u32,
    end_line: u32,
    #[serde(default)]
    side: CommentSide,
    context: CommentContext,
    fingerprint: Option<String>,
    body: String,
    state: CommentState,
    created_at: String,
    updated_at: String,
}

impl ReviewComment {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    pub fn repo_root(&self) -> &str {
        &self.repo_root
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn lines(&self) -> RangeInclusive<u32> {
        self.start_line..=self.end_line
    }

    pub fn side(&self) -> CommentSide {
        self.side
    }

    pub fn context(&self) -> &CommentContext {
        &self.context
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn state(&self) -> &CommentState {
        &self.state
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
}
