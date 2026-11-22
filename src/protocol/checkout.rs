use serde::{Deserialize, Deserializer, Serialize};

use crate::protocol::diff::CheckoutError;
use crate::protocol::number::WireNumber;
use crate::protocol::recognised::{recognised, recognised_entries};

const GITLAB_MERGEABLE: &str = "mergeable";
const GITLAB_LEGACY_MERGEABLE: &str = "can_be_merged";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Merge,
    Squash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedPullRequest {
    url: String,
    number: Option<u64>,
}

impl CreatedPullRequest {
    pub(crate) fn new(url: String, number: Option<u64>) -> CreatedPullRequest {
        CreatedPullRequest { url, number }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn number(&self) -> Option<u64> {
        self.number
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PullRequestTimelineItem {
    Comment(PullRequestComment),
    Review(PullRequestReview),
}

fn milliseconds(created_at: Option<WireNumber>) -> Option<u64> {
    created_at
        .map(WireNumber::count)
        .filter(|milliseconds| *milliseconds > 0)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReview {
    #[serde(default)]
    id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    author_url: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: Option<WireNumber>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    review_state: Option<ReviewState>,
}

impl PullRequestReview {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn author_url(&self) -> Option<&str> {
        self.author_url.as_deref()
    }

    pub fn avatar_url(&self) -> Option<&str> {
        self.avatar_url.as_deref()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    /// Milliseconds since the Unix epoch.
    pub fn created_at(&self) -> Option<u64> {
        milliseconds(self.created_at)
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref().filter(|url| !url.is_empty())
    }

    pub fn review_state(&self) -> ReviewState {
        self.review_state.unwrap_or(ReviewState::Commented)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestComment {
    #[serde(default)]
    id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    author_url: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: Option<WireNumber>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    review_id: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    thread_is_resolved: Option<bool>,
    #[serde(default)]
    location: Option<PullRequestCommentLocation>,
}

impl PullRequestComment {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn author_url(&self) -> Option<&str> {
        self.author_url.as_deref()
    }

    pub fn avatar_url(&self) -> Option<&str> {
        self.avatar_url.as_deref()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    /// Milliseconds since the Unix epoch.
    pub fn created_at(&self) -> Option<u64> {
        milliseconds(self.created_at)
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref().filter(|url| !url.is_empty())
    }

    pub fn review_id(&self) -> Option<&str> {
        self.review_id.as_deref()
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub fn thread_is_resolved(&self) -> Option<bool> {
        self.thread_is_resolved
    }

    pub fn location(&self) -> Option<&PullRequestCommentLocation> {
        self.location.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommentLocation {
    path: String,
    #[serde(default)]
    line: Option<WireNumber>,
    #[serde(default)]
    start_line: Option<WireNumber>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    is_resolved: Option<bool>,
    #[serde(default)]
    is_outdated: Option<bool>,
}

impl PullRequestCommentLocation {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn line(&self) -> Option<u32> {
        self.line.and_then(WireNumber::whole_u32)
    }

    pub fn start_line(&self) -> Option<u32> {
        self.start_line.and_then(WireNumber::whole_u32)
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub fn is_resolved(&self) -> Option<bool> {
        self.is_resolved
    }

    pub fn is_outdated(&self) -> Option<bool> {
        self.is_outdated
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimelineErrorKind {
    Forbidden,
    NotFound,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TimelineError {
    #[serde(default)]
    kind: Option<TimelineErrorKind>,
    #[serde(default)]
    message: String,
}

impl TimelineError {
    pub fn kind(&self) -> TimelineErrorKind {
        self.kind.unwrap_or(TimelineErrorKind::Unknown)
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestTimeline {
    #[serde(default)]
    pr_number: Option<WireNumber>,
    #[serde(default, deserialize_with = "recognised_entries")]
    items: Vec<PullRequestTimelineItem>,
    #[serde(default)]
    truncated: bool,
    #[serde(default, deserialize_with = "recognised")]
    error: Option<TimelineError>,
    #[serde(default)]
    github_features_enabled: Option<bool>,
    #[serde(default, deserialize_with = "recognised")]
    auth_state: Option<ForgeAuthState>,
}

impl PullRequestTimeline {
    pub fn pr_number(&self) -> Option<u64> {
        self.pr_number.map(WireNumber::count)
    }

    pub fn items(&self) -> &[PullRequestTimelineItem] {
        &self.items
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn error(&self) -> Option<&TimelineError> {
        self.error.as_ref()
    }

    pub fn auth_state(&self) -> ForgeAuthState {
        ForgeAuthState::reported(
            self.auth_state,
            self.github_features_enabled.unwrap_or(true),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Rebase,
    Squash,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommitFileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CommitFile {
    path: String,
    additions: WireNumber,
    deletions: WireNumber,
    #[serde(default)]
    status: Option<CommitFileStatus>,
}

impl CommitFile {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn additions(&self) -> u64 {
        self.additions.count()
    }

    pub fn deletions(&self) -> u64 {
        self.deletions.count()
    }

    pub fn status(&self) -> Option<CommitFileStatus> {
        self.status
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    #[serde(flatten)]
    summary: CommitSummary,
    files: Vec<CommitFile>,
}

impl Commit {
    pub fn summary(&self) -> &CommitSummary {
        &self.summary
    }

    pub fn files(&self) -> &[CommitFile] {
        &self.files
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    sha: String,
    short_sha: String,
    subject: String,
    author_name: String,
    author_date: String,
    is_on_remote: bool,
    #[serde(default)]
    is_on_base: Option<bool>,
}

impl CommitSummary {
    pub fn sha(&self) -> &str {
        &self.sha
    }

    pub fn short_sha(&self) -> &str {
        &self.short_sha
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn author_name(&self) -> &str {
        &self.author_name
    }

    pub fn author_date(&self) -> &str {
        &self.author_date
    }

    /// Whether the commit has been pushed.
    pub fn is_on_remote(&self) -> bool {
        self.is_on_remote
    }

    /// Whether the commit is already on the base branch; `None` from daemons that don't classify.
    pub fn is_on_base(&self) -> Option<bool> {
        self.is_on_base
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitList {
    base_ref: Option<String>,
    commits: Vec<Commit>,
}

impl CommitList {
    pub fn base_ref(&self) -> Option<&str> {
        self.base_ref.as_deref()
    }

    pub fn commits(&self) -> &[Commit] {
        &self.commits
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitPage {
    base_ref: Option<String>,
    commits: Vec<CommitSummary>,
    next_cursor: Option<String>,
}

impl CommitPage {
    pub fn base_ref(&self) -> Option<&str> {
        self.base_ref.as_deref()
    }

    pub fn commits(&self) -> &[CommitSummary] {
        &self.commits
    }

    /// The cursor of the next page; `None` once the history is exhausted.
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BranchSource {
    Local,
    Remote,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    index: u32,
    message: String,
    branch: Option<String>,
    is_paseo: bool,
}

impl StashEntry {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn is_paseo(&self) -> bool {
        self.is_paseo
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchValidation {
    exists: bool,
    resolved_ref: Option<String>,
    is_remote: bool,
}

impl BranchValidation {
    pub fn exists(&self) -> bool {
        self.exists
    }

    pub fn resolved_ref(&self) -> Option<&str> {
        self.resolved_ref.as_deref()
    }

    pub fn is_remote(&self) -> bool {
        self.is_remote
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDetailsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_run_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_run_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_request_number: Option<u64>,
}

impl CheckDetailsQuery {
    pub fn repo_owner(&self) -> Option<&str> {
        self.repo_owner.as_deref()
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.repo_name.as_deref()
    }

    pub fn set_repository(&mut self, owner: impl Into<String>, name: impl Into<String>) {
        self.repo_owner = Some(owner.into());
        self.repo_name = Some(name.into());
    }

    pub fn with_repository(
        mut self,
        owner: impl Into<String>,
        name: impl Into<String>,
    ) -> CheckDetailsQuery {
        self.set_repository(owner, name);
        self
    }

    pub fn check_run_id(&self) -> Option<u64> {
        self.check_run_id
    }

    pub fn set_check_run_id(&mut self, check_run_id: u64) {
        self.check_run_id = Some(check_run_id);
    }

    pub fn with_check_run_id(mut self, check_run_id: u64) -> CheckDetailsQuery {
        self.set_check_run_id(check_run_id);
        self
    }

    pub fn workflow_run_id(&self) -> Option<u64> {
        self.workflow_run_id
    }

    pub fn set_workflow_run_id(&mut self, workflow_run_id: u64) {
        self.workflow_run_id = Some(workflow_run_id);
    }

    pub fn with_workflow_run_id(mut self, workflow_run_id: u64) -> CheckDetailsQuery {
        self.set_workflow_run_id(workflow_run_id);
        self
    }

    pub fn change_request_number(&self) -> Option<u64> {
        self.change_request_number
    }

    pub fn set_change_request_number(&mut self, number: u64) {
        self.change_request_number = Some(number);
    }

    pub fn with_change_request_number(mut self, number: u64) -> CheckDetailsQuery {
        self.set_change_request_number(number);
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CheckOutput {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

impl CheckOutput {
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckAnnotation {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    start_line: Option<WireNumber>,
    #[serde(default)]
    end_line: Option<WireNumber>,
    #[serde(default)]
    annotation_level: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    raw_details: Option<String>,
}

impl CheckAnnotation {
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn start_line(&self) -> Option<u32> {
        self.start_line.and_then(WireNumber::whole_u32)
    }

    pub fn end_line(&self) -> Option<u32> {
        self.end_line.and_then(WireNumber::whole_u32)
    }

    pub fn annotation_level(&self) -> Option<&str> {
        self.annotation_level.as_deref()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn raw_details(&self) -> Option<&str> {
        self.raw_details.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckJob {
    job_id: WireNumber,
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    log_tail: Option<String>,
    #[serde(default)]
    log_truncated: bool,
}

impl CheckJob {
    pub fn job_id(&self) -> u64 {
        self.job_id.count()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn conclusion(&self) -> Option<&str> {
        self.conclusion.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn log_tail(&self) -> Option<&str> {
        self.log_tail.as_deref()
    }

    pub fn log_truncated(&self) -> bool {
        self.log_truncated
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    #[serde(alias = "cancelled")]
    Canceled,
    Canceling,
    Created,
    Failed,
    Manual,
    Pending,
    Preparing,
    Running,
    Scheduled,
    Skipped,
    #[serde(alias = "passed")]
    Success,
    WaitingForResource,
    #[serde(other)]
    Unknown,
}

impl PipelineStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            PipelineStatus::Canceling
                | PipelineStatus::Created
                | PipelineStatus::Pending
                | PipelineStatus::Preparing
                | PipelineStatus::Running
                | PipelineStatus::Scheduled
                | PipelineStatus::WaitingForResource
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PipelineJob {
    id: WireNumber,
    name: String,
    stage: String,
    status: PipelineStatus,
    #[serde(default)]
    raw_status: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    allow_failure: bool,
    #[serde(default)]
    duration_seconds: Option<f64>,
}

impl PipelineJob {
    pub fn id(&self) -> u64 {
        self.id.count()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stage(&self) -> &str {
        &self.stage
    }

    pub fn status(&self) -> PipelineStatus {
        self.status
    }

    pub fn raw_status(&self) -> Option<&str> {
        self.raw_status.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn allow_failure(&self) -> bool {
        self.allow_failure
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        self.duration_seconds
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PipelineStage {
    name: String,
    status: PipelineStatus,
    #[serde(default)]
    jobs: Vec<PipelineJob>,
}

impl PipelineStage {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> PipelineStatus {
        self.status
    }

    pub fn jobs(&self) -> &[PipelineJob] {
        &self.jobs
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    id: WireNumber,
    status: PipelineStatus,
    #[serde(default)]
    raw_status: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(rename = "ref", default)]
    git_ref: Option<String>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    stages: Vec<PipelineStage>,
}

impl Pipeline {
    pub fn id(&self) -> u64 {
        self.id.count()
    }

    pub fn status(&self) -> PipelineStatus {
        self.status
    }

    pub fn raw_status(&self) -> Option<&str> {
        self.raw_status.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn git_ref(&self) -> Option<&str> {
        self.git_ref.as_deref()
    }

    pub fn sha(&self) -> Option<&str> {
        self.sha.as_deref()
    }

    pub fn stages(&self) -> &[PipelineStage] {
        &self.stages
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckDetails {
    check_run_id: WireNumber,
    #[serde(default)]
    workflow_run_id: Option<WireNumber>,
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    details_url: Option<String>,
    #[serde(default)]
    output: Option<CheckOutput>,
    #[serde(default)]
    annotations: Vec<CheckAnnotation>,
    #[serde(default)]
    failed_jobs: Vec<CheckJob>,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    pipeline: Option<Pipeline>,
}

impl CheckDetails {
    pub fn check_run_id(&self) -> u64 {
        self.check_run_id.count()
    }

    pub fn workflow_run_id(&self) -> Option<u64> {
        self.workflow_run_id.map(WireNumber::count)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn conclusion(&self) -> Option<&str> {
        self.conclusion.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn details_url(&self) -> Option<&str> {
        self.details_url.as_deref()
    }

    pub fn output(&self) -> Option<&CheckOutput> {
        self.output.as_ref()
    }

    pub fn annotations(&self) -> &[CheckAnnotation] {
        &self.annotations
    }

    pub fn failed_jobs(&self) -> &[CheckJob] {
        &self.failed_jobs
    }

    /// Whether the daemon cut annotations or logs short.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn pipeline(&self) -> Option<&Pipeline> {
        self.pipeline.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeItemKind {
    #[serde(alias = "pr")]
    ChangeRequest,
    Issue,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForgeItem {
    kind: ForgeItemKind,
    #[serde(default)]
    forge: Option<String>,
    number: WireNumber,
    title: String,
    url: String,
    state: String,
    body: Option<String>,
    labels: Vec<String>,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default)]
    base_ref_name: Option<String>,
    #[serde(default)]
    head_ref_name: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

impl ForgeItem {
    pub fn kind(&self) -> ForgeItemKind {
        self.kind
    }

    pub fn forge(&self) -> Option<&str> {
        self.forge.as_deref()
    }

    pub fn number(&self) -> u64 {
        self.number.count()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
    }

    pub fn base_ref_name(&self) -> Option<&str> {
        self.base_ref_name.as_deref()
    }

    pub fn head_ref_name(&self) -> Option<&str> {
        self.head_ref_name.as_deref()
    }

    pub fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ForgeAuthState {
    Authenticated,
    CliMissing,
    Error,
    NoRemote,
    Unauthenticated,
    #[serde(other)]
    Unknown,
}

impl ForgeAuthState {
    pub(crate) fn unreported(features_enabled: bool) -> ForgeAuthState {
        if features_enabled {
            ForgeAuthState::Authenticated
        } else {
            ForgeAuthState::Unauthenticated
        }
    }

    fn reported(reported: Option<ForgeAuthState>, features_enabled: bool) -> ForgeAuthState {
        reported
            .filter(|state| *state != ForgeAuthState::Unknown)
            .unwrap_or_else(|| ForgeAuthState::unreported(features_enabled))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForgeSearchResults {
    items: Vec<ForgeItem>,
    auth_state: ForgeAuthState,
}

impl ForgeSearchResults {
    pub(crate) fn new(items: Vec<ForgeItem>, auth_state: ForgeAuthState) -> ForgeSearchResults {
        ForgeSearchResults { items, auth_state }
    }

    pub fn items(&self) -> &[ForgeItem] {
        &self.items
    }

    pub fn auth_state(&self) -> ForgeAuthState {
        self.auth_state
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryVisibility {
    Internal,
    Private,
    Public,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepository {
    id: String,
    name: String,
    name_with_owner: String,
    description: Option<String>,
    visibility: RepositoryVisibility,
    updated_at: String,
    clone_url: String,
}

impl GithubRepository {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn name_with_owner(&self) -> &str {
        &self.name_with_owner
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn visibility(&self) -> RepositoryVisibility {
        self.visibility
    }

    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    pub fn clone_url(&self) -> &str {
        &self.clone_url
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RepositorySearch {
    Error {
        repositories: Vec<GithubRepository>,
        error: String,
    },
    Success {
        repositories: Vec<GithubRepository>,
    },
    Unauthenticated {
        error: String,
    },
    Unavailable {
        error: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Mergeability {
    Conflicting,
    Mergeable,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Cancelled,
    Failure,
    Pending,
    Skipped,
    Success,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksSummary {
    Failure,
    None,
    Pending,
    Success,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    Pending,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCheck {
    name: String,
    status: CheckStatus,
    url: Option<String>,
    #[serde(default)]
    workflow: Option<String>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    check_run_id: Option<WireNumber>,
    #[serde(default)]
    workflow_run_id: Option<WireNumber>,
    #[serde(default)]
    traits: Vec<String>,
}

impl PullRequestCheck {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> CheckStatus {
        self.status
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn workflow(&self) -> Option<&str> {
        self.workflow.as_deref()
    }

    pub fn duration(&self) -> Option<&str> {
        self.duration.as_deref()
    }

    pub fn check_run_id(&self) -> Option<u64> {
        self.check_run_id.map(WireNumber::count)
    }

    pub fn workflow_run_id(&self) -> Option<u64> {
        self.workflow_run_id.map(WireNumber::count)
    }

    pub fn traits(&self) -> &[String] {
        &self.traits
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequest {
    #[serde(default)]
    number: Option<WireNumber>,
    url: String,
    title: String,
    state: String,
    base_ref_name: String,
    head_ref_name: String,
    is_merged: bool,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    mergeable: Option<Mergeability>,
    #[serde(default)]
    checks: Vec<PullRequestCheck>,
    #[serde(default)]
    checks_status: Option<ChecksSummary>,
    #[serde(default)]
    review_decision: Option<ReviewDecision>,
    #[serde(default)]
    repo_owner: Option<String>,
    #[serde(default)]
    repo_name: Option<String>,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default, deserialize_with = "recognised")]
    forge_specific: Option<ForgeFacts>,
    #[serde(default, deserialize_with = "github_mirror")]
    github: Option<ForgeFacts>,
}

impl WorkspacePullRequest {
    pub fn number(&self) -> Option<u64> {
        self.number.map(WireNumber::count)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn base_ref_name(&self) -> &str {
        &self.base_ref_name
    }

    pub fn head_ref_name(&self) -> &str {
        &self.head_ref_name
    }

    pub fn is_merged(&self) -> bool {
        self.is_merged
    }

    pub fn is_draft(&self) -> bool {
        self.is_draft
    }

    pub fn mergeable(&self) -> Option<Mergeability> {
        self.mergeable
    }

    pub fn checks(&self) -> &[PullRequestCheck] {
        &self.checks
    }

    pub fn checks_status(&self) -> Option<ChecksSummary> {
        self.checks_status
    }

    pub fn review_decision(&self) -> Option<ReviewDecision> {
        self.review_decision
    }

    pub fn repo_owner(&self) -> Option<&str> {
        self.repo_owner.as_deref()
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.repo_name.as_deref()
    }

    pub fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
    }

    pub fn forge_facts(&self) -> Option<&ForgeFacts> {
        self.forge_specific.as_ref().or(self.github.as_ref())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct ForgeRuntimeError {
    message: String,
}

/// The forge state the daemon polls for a workspace's branch: its pull request and checks.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForgeRuntime {
    #[serde(default)]
    features_enabled: Option<bool>,
    #[serde(default)]
    pull_request: Option<WorkspacePullRequest>,
    #[serde(default)]
    error: Option<ForgeRuntimeError>,
    #[serde(default)]
    refreshed_at: Option<String>,
}

impl ForgeRuntime {
    pub fn features_enabled(&self) -> Option<bool> {
        self.features_enabled
    }

    pub fn pull_request(&self) -> Option<&WorkspacePullRequest> {
        self.pull_request.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(|error| error.message.as_str())
    }

    pub fn refreshed_at(&self) -> Option<&str> {
        self.refreshed_at.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForgeStatus {
    #[serde(default)]
    status: Option<WorkspacePullRequest>,
    #[serde(default)]
    github_features_enabled: bool,
    #[serde(default, deserialize_with = "recognised")]
    auth_state: Option<ForgeAuthState>,
    #[serde(default)]
    forge: Option<String>,
    #[serde(default)]
    error: Option<CheckoutError>,
}

impl ForgeStatus {
    pub fn pull_request(&self) -> Option<&WorkspacePullRequest> {
        self.status.as_ref()
    }

    pub fn features_enabled(&self) -> bool {
        self.github_features_enabled
    }

    pub fn auth_state(&self) -> ForgeAuthState {
        ForgeAuthState::reported(self.auth_state, self.github_features_enabled)
    }

    pub fn forge(&self) -> Option<&str> {
        self.forge.as_deref().filter(|forge| !forge.is_empty())
    }

    pub fn error(&self) -> Option<&CheckoutError> {
        self.error.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "forge", rename_all = "lowercase")]
pub enum ForgeFacts {
    Gitea(GiteaFacts),
    Github(GithubFacts),
    Gitlab(GitlabFacts),
    #[serde(other)]
    Unknown,
}

fn github_mirror<'de, D>(deserializer: D) -> Result<Option<ForgeFacts>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(recognised::<_, GithubFacts>(deserializer)?.map(ForgeFacts::Github))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeStateStatus {
    Behind,
    Blocked,
    Clean,
    Dirty,
    Draft,
    HasHooks,
    Unstable,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GithubMergeMethod {
    Merge,
    Rebase,
    Squash,
    #[serde(other)]
    Unknown,
}

impl GithubMergeMethod {
    pub fn merge_method(self) -> Option<MergeMethod> {
        match self {
            GithubMergeMethod::Merge => Some(MergeMethod::Merge),
            GithubMergeMethod::Rebase => Some(MergeMethod::Rebase),
            GithubMergeMethod::Squash => Some(MergeMethod::Squash),
            GithubMergeMethod::Unknown => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoMergeRequest {
    #[serde(default)]
    enabled_at: Option<String>,
    #[serde(default)]
    merge_method: Option<GithubMergeMethod>,
    #[serde(default)]
    enabled_by: Option<String>,
}

impl AutoMergeRequest {
    pub fn enabled_at(&self) -> Option<&str> {
        self.enabled_at.as_deref()
    }

    pub fn merge_method(&self) -> Option<MergeMethod> {
        self.merge_method.and_then(GithubMergeMethod::merge_method)
    }

    pub fn enabled_by(&self) -> Option<&str> {
        self.enabled_by.as_deref()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GithubRepositoryPolicy {
    auto_merge_allowed: bool,
    merge_commit_allowed: bool,
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
    viewer_default_merge_method: Option<GithubMergeMethod>,
}

impl GithubRepositoryPolicy {
    pub fn auto_merge_allowed(&self) -> bool {
        self.auto_merge_allowed
    }

    pub fn allows(&self, method: MergeMethod) -> bool {
        match method {
            MergeMethod::Merge => self.merge_commit_allowed,
            MergeMethod::Rebase => self.rebase_merge_allowed,
            MergeMethod::Squash => self.squash_merge_allowed,
        }
    }

    pub fn viewer_default_merge_method(&self) -> Option<MergeMethod> {
        self.viewer_default_merge_method
            .and_then(GithubMergeMethod::merge_method)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GithubFacts {
    merge_state_status: Option<MergeStateStatus>,
    auto_merge_request: Option<AutoMergeRequest>,
    viewer_can_enable_auto_merge: bool,
    viewer_can_disable_auto_merge: bool,
    repository: GithubRepositoryPolicy,
    is_merge_queue_enabled: bool,
    is_in_merge_queue: bool,
}

impl GithubFacts {
    pub fn merge_state_status(&self) -> Option<MergeStateStatus> {
        self.merge_state_status
    }

    pub fn auto_merge_request(&self) -> Option<&AutoMergeRequest> {
        self.auto_merge_request.as_ref()
    }

    pub fn viewer_can_enable_auto_merge(&self) -> bool {
        self.viewer_can_enable_auto_merge
    }

    pub fn viewer_can_disable_auto_merge(&self) -> bool {
        self.viewer_can_disable_auto_merge
    }

    pub fn repository(&self) -> &GithubRepositoryPolicy {
        &self.repository
    }

    pub fn is_merge_queue_enabled(&self) -> bool {
        self.is_merge_queue_enabled
    }

    pub fn is_in_merge_queue(&self) -> bool {
        self.is_in_merge_queue
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitlabFacts {
    #[serde(default)]
    detailed_merge_status: Option<String>,
    #[serde(default)]
    merge_status: Option<String>,
    #[serde(default)]
    has_conflicts: bool,
    #[serde(default)]
    pipeline_status: Option<PipelineStatus>,
    #[serde(default)]
    pipeline_id: Option<WireNumber>,
    #[serde(default)]
    pipeline_url: Option<String>,
    #[serde(default)]
    merge_when_pipeline_succeeds: bool,
}

impl GitlabFacts {
    pub fn is_mergeable(&self) -> bool {
        match &self.detailed_merge_status {
            Some(status) => status == GITLAB_MERGEABLE,
            None => {
                self.merge_status.as_deref() == Some(GITLAB_LEGACY_MERGEABLE) && !self.has_conflicts
            }
        }
    }

    pub fn has_conflicts(&self) -> bool {
        self.has_conflicts
    }

    pub fn pipeline_status(&self) -> Option<PipelineStatus> {
        self.pipeline_status
    }

    pub fn pipeline_id(&self) -> Option<u64> {
        self.pipeline_id.map(WireNumber::count)
    }

    pub fn pipeline_url(&self) -> Option<&str> {
        self.pipeline_url.as_deref()
    }

    pub fn merge_when_pipeline_succeeds(&self) -> bool {
        self.merge_when_pipeline_succeeds
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GiteaFacts {
    #[serde(default)]
    mergeable: bool,
    #[serde(default)]
    has_merged: bool,
}

impl GiteaFacts {
    pub fn mergeable(&self) -> bool {
        self.mergeable
    }

    pub fn has_merged(&self) -> bool {
        self.has_merged
    }
}
