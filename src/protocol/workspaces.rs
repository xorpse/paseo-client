use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::error::ArgumentError;
use crate::protocol::agents::{AgentSnapshot, Page, PageInfo, Sort, SortDirection};
use crate::protocol::checkout::ForgeRuntime;
use crate::protocol::number::WireNumber;
use crate::protocol::timeline::WorktreeSetupDetail;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    Directory,
    Git,
    NonGit,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Checkout,
    Directory,
    LocalCheckout,
    Worktree,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Attention,
    Done,
    Failed,
    NeedsInput,
    Running,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct DiffStat {
    additions: WireNumber,
    deletions: WireNumber,
}

impl DiffStat {
    pub fn additions(&self) -> u64 {
        self.additions.count()
    }

    pub fn deletions(&self) -> u64 {
        self.deletions.count()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
    Script,
    #[default]
    Service,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLifecycle {
    Running,
    Stopped,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptHealth {
    Healthy,
    Unhealthy,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceScript {
    script_name: String,
    #[serde(rename = "type", default)]
    kind: ScriptKind,
    hostname: String,
    port: Option<NonZeroU64>,
    #[serde(default)]
    local_proxy_url: Option<String>,
    #[serde(default)]
    public_proxy_url: Option<String>,
    #[serde(default)]
    proxy_url: Option<String>,
    lifecycle: ScriptLifecycle,
    health: Option<ScriptHealth>,
    #[serde(default)]
    exit_code: Option<WireNumber>,
    #[serde(default)]
    terminal_id: Option<String>,
}

impl WorkspaceScript {
    pub fn script_name(&self) -> &str {
        &self.script_name
    }

    pub fn kind(&self) -> ScriptKind {
        self.kind
    }

    pub fn is_service(&self) -> bool {
        self.kind == ScriptKind::Service
    }

    pub fn is_running(&self) -> bool {
        self.lifecycle == ScriptLifecycle::Running
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn port(&self) -> Option<u16> {
        self.port.and_then(|port| u16::try_from(port.get()).ok())
    }

    pub fn local_proxy_url(&self) -> Option<&str> {
        self.local_proxy_url.as_deref()
    }

    pub fn public_proxy_url(&self) -> Option<&str> {
        self.public_proxy_url.as_deref()
    }

    pub fn proxy_url(&self) -> Option<&str> {
        self.proxy_url.as_deref()
    }

    pub fn lifecycle(&self) -> ScriptLifecycle {
        self.lifecycle
    }

    pub fn health(&self) -> Option<ScriptHealth> {
        self.health
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.and_then(WireNumber::whole_i32)
    }

    pub fn terminal_id(&self) -> Option<&str> {
        self.terminal_id.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct AheadBehind {
    ahead: WireNumber,
    behind: WireNumber,
}

impl AheadBehind {
    pub fn ahead(&self) -> u64 {
        self.ahead.count()
    }

    pub fn behind(&self) -> u64 {
        self.behind.count()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitRuntime {
    #[serde(default)]
    current_branch: Option<String>,
    #[serde(default)]
    remote_url: Option<String>,
    #[serde(default)]
    is_paseo_owned_worktree: bool,
    #[serde(default)]
    is_dirty: Option<bool>,
    #[serde(default)]
    ahead_behind: Option<AheadBehind>,
    #[serde(default)]
    ahead_of_origin: Option<WireNumber>,
    #[serde(default)]
    behind_of_origin: Option<WireNumber>,
}

impl WorkspaceGitRuntime {
    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    pub fn remote_url(&self) -> Option<&str> {
        self.remote_url.as_deref()
    }

    pub fn is_paseo_owned_worktree(&self) -> bool {
        self.is_paseo_owned_worktree
    }

    pub fn is_dirty(&self) -> Option<bool> {
        self.is_dirty
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(from = "ProjectCheckoutWire")]
pub enum ProjectCheckout {
    Directory(DirectoryCheckout),
    Git(GitCheckout),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryCheckout {
    cwd: String,
}

impl DirectoryCheckout {
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitCheckout {
    cwd: String,
    current_branch: Option<String>,
    remote_url: Option<String>,
    worktree_root: String,
    is_paseo_owned_worktree: bool,
    main_repo_root: Option<String>,
}

impl GitCheckout {
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    pub fn remote_url(&self) -> Option<&str> {
        self.remote_url.as_deref()
    }

    pub fn worktree_root(&self) -> &str {
        &self.worktree_root
    }

    pub fn is_paseo_owned_worktree(&self) -> bool {
        self.is_paseo_owned_worktree
    }

    pub fn main_repo_root(&self) -> Option<&str> {
        self.main_repo_root.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCheckoutWire {
    cwd: String,
    is_git: bool,
    #[serde(default)]
    current_branch: Option<String>,
    #[serde(default)]
    remote_url: Option<String>,
    #[serde(default)]
    worktree_root: Option<String>,
    #[serde(default)]
    is_paseo_owned_worktree: bool,
    #[serde(default)]
    main_repo_root: Option<String>,
}

impl From<ProjectCheckoutWire> for ProjectCheckout {
    fn from(wire: ProjectCheckoutWire) -> ProjectCheckout {
        if !wire.is_git {
            return ProjectCheckout::Directory(DirectoryCheckout { cwd: wire.cwd });
        }
        ProjectCheckout::Git(GitCheckout {
            worktree_root: wire.worktree_root.unwrap_or_else(|| wire.cwd.clone()),
            cwd: wire.cwd,
            current_branch: wire.current_branch,
            remote_url: wire.remote_url,
            is_paseo_owned_worktree: wire.is_paseo_owned_worktree,
            main_repo_root: wire.main_repo_root,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPlacement {
    project_key: String,
    project_name: String,
    #[serde(default)]
    workspace_name: Option<String>,
    checkout: ProjectCheckout,
}

impl ProjectPlacement {
    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn workspace_name(&self) -> Option<&str> {
        self.workspace_name.as_deref()
    }

    pub fn checkout(&self) -> &ProjectCheckout {
        &self.checkout
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDescriptor {
    project_id: String,
    #[serde(default)]
    project_key: Option<String>,
    project_display_name: String,
    #[serde(default)]
    project_custom_name: Option<String>,
    #[serde(default)]
    project_custom_icon_revision: Option<String>,
    #[serde(default)]
    project_icon_revision: Option<String>,
    project_root_path: String,
    project_kind: ProjectKind,
    #[serde(default)]
    sync_seq: Option<u64>,
}

impl ProjectDescriptor {
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn project_key(&self) -> Option<&str> {
        self.project_key.as_deref()
    }

    pub fn project_display_name(&self) -> &str {
        &self.project_display_name
    }

    pub fn project_custom_name(&self) -> Option<&str> {
        self.project_custom_name.as_deref()
    }

    pub fn project_custom_icon_revision(&self) -> Option<&str> {
        self.project_custom_icon_revision.as_deref()
    }

    pub fn project_icon_revision(&self) -> Option<&str> {
        self.project_icon_revision.as_deref()
    }

    pub fn project_root_path(&self) -> &str {
        &self.project_root_path
    }

    pub fn project_kind(&self) -> ProjectKind {
        self.project_kind
    }

    pub fn sync_seq(&self) -> Option<u64> {
        self.sync_seq
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    id: String,
    project_id: String,
    project_display_name: String,
    #[serde(default)]
    project_custom_name: Option<String>,
    #[serde(default)]
    project_custom_icon_revision: Option<String>,
    project_root_path: String,
    #[serde(default)]
    workspace_directory: Option<String>,
    #[serde(default)]
    setup_status: Option<SetupStatus>,
    #[serde(default)]
    worktree_slug: Option<String>,
    project_kind: ProjectKind,
    workspace_kind: WorkspaceKind,
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    pinned_at: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    background: bool,
    #[serde(default)]
    archiving_at: Option<String>,
    status: WorkspaceStatus,
    #[serde(default)]
    status_entered_at: Option<String>,
    activity_at: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    last_user_message_at: Option<String>,
    #[serde(default)]
    diff_stat: Option<DiffStat>,
    #[serde(default)]
    scripts: Vec<WorkspaceScript>,
    #[serde(default)]
    git_runtime: Option<WorkspaceGitRuntime>,
    #[serde(default, rename = "githubRuntime")]
    forge_runtime: Option<ForgeRuntime>,
    #[serde(default)]
    forge: Option<String>,
    #[serde(default)]
    project: Option<ProjectPlacement>,
    #[serde(default)]
    sync_seq: Option<u64>,
}

impl WorkspaceDescriptor {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn project_display_name(&self) -> &str {
        &self.project_display_name
    }

    pub fn project_custom_name(&self) -> Option<&str> {
        self.project_custom_name.as_deref()
    }

    pub fn project_root_path(&self) -> &str {
        &self.project_root_path
    }

    pub fn project_custom_icon_revision(&self) -> Option<&str> {
        self.project_custom_icon_revision.as_deref()
    }

    pub fn workspace_directory(&self) -> Option<&str> {
        self.workspace_directory.as_deref()
    }

    pub fn setup_status(&self) -> Option<SetupStatus> {
        self.setup_status
    }

    pub fn worktree_slug(&self) -> Option<&str> {
        self.worktree_slug.as_deref()
    }

    pub fn project_kind(&self) -> ProjectKind {
        self.project_kind
    }

    pub fn workspace_kind(&self) -> WorkspaceKind {
        self.workspace_kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn pinned_at(&self) -> Option<&str> {
        self.pinned_at.as_deref()
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn background(&self) -> bool {
        self.background
    }

    pub fn archiving_at(&self) -> Option<&str> {
        self.archiving_at.as_deref()
    }

    pub fn status(&self) -> WorkspaceStatus {
        self.status
    }

    pub fn status_entered_at(&self) -> Option<&str> {
        self.status_entered_at.as_deref()
    }

    pub fn activity_at(&self) -> Option<&str> {
        self.activity_at.as_deref()
    }

    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    pub fn last_user_message_at(&self) -> Option<&str> {
        self.last_user_message_at.as_deref()
    }

    pub fn diff_stat(&self) -> Option<DiffStat> {
        self.diff_stat
    }

    pub fn scripts(&self) -> &[WorkspaceScript] {
        &self.scripts
    }

    pub fn set_scripts(&mut self, scripts: Vec<WorkspaceScript>) {
        self.scripts = scripts;
    }

    pub fn git_runtime(&self) -> Option<&WorkspaceGitRuntime> {
        self.git_runtime.as_ref()
    }

    pub fn forge_runtime(&self) -> Option<&ForgeRuntime> {
        self.forge_runtime.as_ref()
    }

    pub fn forge(&self) -> Option<&str> {
        self.forge.as_deref()
    }

    pub fn project(&self) -> Option<&ProjectPlacement> {
        self.project.as_ref()
    }

    pub fn sync_seq(&self) -> Option<u64> {
        self.sync_seq
    }

    pub fn directory(&self) -> &str {
        self.workspace_directory
            .as_deref()
            .unwrap_or(&self.project_root_path)
    }

    pub fn branch(&self) -> Option<&str> {
        self.git_runtime.as_ref()?.current_branch.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirectoryPage {
    entries: Vec<WorkspaceDescriptor>,
    #[serde(default)]
    empty_projects: Vec<ProjectDescriptor>,
    page_info: PageInfo,
}

impl WorkspaceDirectoryPage {
    pub fn entries(&self) -> &[WorkspaceDescriptor] {
        &self.entries
    }

    pub fn empty_projects(&self) -> &[ProjectDescriptor] {
        &self.empty_projects
    }

    pub fn page_info(&self) -> &PageInfo {
        &self.page_info
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceUpdate {
    Remove(WorkspaceRemoval),
    Upsert(WorkspaceUpsert),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRemoval {
    id: String,
    #[serde(default)]
    empty_project: Option<Box<ProjectDescriptor>>,
    #[serde(default)]
    removed_project_id: Option<String>,
}

impl WorkspaceRemoval {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn empty_project(&self) -> Option<&ProjectDescriptor> {
        self.empty_project.as_deref()
    }

    pub fn removed_project_id(&self) -> Option<&str> {
        self.removed_project_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WorkspaceUpsert {
    workspace: Box<WorkspaceDescriptor>,
}

impl WorkspaceUpsert {
    pub fn workspace(&self) -> &WorkspaceDescriptor {
        &self.workspace
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectUpdate {
    Remove(ProjectRemoval),
    Upsert(ProjectUpsert),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemoval {
    project_id: String,
}

impl ProjectRemoval {
    pub fn project_id(&self) -> &str {
        &self.project_id
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProjectUpsert {
    project: ProjectDescriptor,
}

impl ProjectUpsert {
    pub fn project(&self) -> &ProjectDescriptor {
        &self.project
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptStatusUpdate {
    workspace_id: String,
    scripts: Vec<WorkspaceScript>,
}

impl ScriptStatusUpdate {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn scripts(&self) -> &[WorkspaceScript] {
        &self.scripts
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SetupStatus {
    Blocked,
    Completed,
    Failed,
    Running,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockedSource {
    forge: String,
    number: NonZeroU64,
    head_repository: String,
}

impl BlockedSource {
    pub fn forge(&self) -> &str {
        &self.forge
    }

    pub fn number(&self) -> NonZeroU64 {
        self.number
    }

    pub fn head_repository(&self) -> &str {
        &self.head_repository
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSetupSnapshot {
    status: SetupStatus,
    detail: WorktreeSetupDetail,
    error: Option<String>,
    #[serde(default)]
    blocked_source: Option<BlockedSource>,
}

impl WorkspaceSetupSnapshot {
    pub fn status(&self) -> SetupStatus {
        self.status
    }

    pub fn detail(&self) -> &WorktreeSetupDetail {
        &self.detail
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn blocked_source(&self) -> Option<&BlockedSource> {
        self.blocked_source.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSetupProgress {
    workspace_id: String,
    #[serde(flatten)]
    snapshot: WorkspaceSetupSnapshot,
}

impl WorkspaceSetupProgress {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn snapshot(&self) -> &WorkspaceSetupSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaseoWorktree {
    worktree_path: String,
    created_at: String,
    #[serde(default)]
    branch_name: Option<String>,
    #[serde(default)]
    head: Option<String>,
}

impl PaseoWorktree {
    pub fn worktree_path(&self) -> &str {
        &self.worktree_path
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn branch_name(&self) -> Option<&str> {
        self.branch_name.as_deref()
    }

    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorktreeAction {
    BranchOff,
    Checkout,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceSource {
    Directory(DirectorySource),
    Worktree(WorktreeSource),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySource {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
}

impl DirectorySource {
    pub fn new(path: impl Into<String>) -> DirectorySource {
        DirectorySource {
            path: path.into(),
            project_id: None,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub fn set_project_id(&mut self, project_id: impl Into<String>) {
        self.project_id = Some(project_id.into());
    }

    pub fn with_project_id(mut self, project_id: impl Into<String>) -> DirectorySource {
        self.set_project_id(project_id);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<WorktreeAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkout_source: Option<ChangeRequestCheckout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_slug: Option<String>,
}

impl WorktreeSource {
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> WorktreeSource {
        self.set_cwd(cwd);
        self
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub fn set_project_id(&mut self, project_id: impl Into<String>) {
        self.project_id = Some(project_id.into());
    }

    pub fn with_project_id(mut self, project_id: impl Into<String>) -> WorktreeSource {
        self.set_project_id(project_id);
        self
    }

    pub fn action(&self) -> Option<WorktreeAction> {
        self.action
    }

    pub fn set_action(&mut self, action: WorktreeAction) {
        self.action = Some(action);
    }

    pub fn with_action(mut self, action: WorktreeAction) -> WorktreeSource {
        self.set_action(action);
        self
    }

    pub fn ref_name(&self) -> Option<&str> {
        self.ref_name.as_deref()
    }

    pub fn set_ref_name(&mut self, ref_name: impl Into<String>) {
        self.ref_name = Some(ref_name.into());
    }

    pub fn with_ref_name(mut self, ref_name: impl Into<String>) -> WorktreeSource {
        self.set_ref_name(ref_name);
        self
    }

    pub fn base_branch(&self) -> Option<&str> {
        self.base_branch.as_deref()
    }

    pub fn set_base_branch(&mut self, base_branch: impl Into<String>) {
        self.base_branch = Some(base_branch.into());
    }

    pub fn with_base_branch(mut self, base_branch: impl Into<String>) -> WorktreeSource {
        self.set_base_branch(base_branch);
        self
    }

    pub fn branch_name(&self) -> Option<&str> {
        self.branch_name.as_deref()
    }

    pub fn set_branch_name(&mut self, branch_name: impl Into<String>) {
        self.branch_name = Some(branch_name.into());
    }

    pub fn with_branch_name(mut self, branch_name: impl Into<String>) -> WorktreeSource {
        self.set_branch_name(branch_name);
        self
    }

    pub fn worktree_slug(&self) -> Option<&str> {
        self.worktree_slug.as_deref()
    }

    pub fn set_worktree_slug(&mut self, worktree_slug: impl Into<String>) {
        self.worktree_slug = Some(worktree_slug.into());
    }

    pub fn with_worktree_slug(mut self, worktree_slug: impl Into<String>) -> WorktreeSource {
        self.set_worktree_slug(worktree_slug);
        self
    }

    pub fn checkout_source(&self) -> Option<&ChangeRequestCheckout> {
        self.checkout_source.as_ref()
    }

    pub fn set_checkout_source(&mut self, checkout_source: ChangeRequestCheckout) {
        self.checkout_source = Some(checkout_source);
    }

    pub fn with_checkout_source(
        mut self,
        checkout_source: ChangeRequestCheckout,
    ) -> WorktreeSource {
        self.set_checkout_source(checkout_source);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename = "change_request", rename_all = "camelCase")]
pub struct ChangeRequestCheckout {
    #[serde(skip_serializing_if = "Option::is_none")]
    forge: Option<String>,
    number: NonZeroU64,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_path: Option<String>,
}

impl ChangeRequestCheckout {
    pub fn new(number: NonZeroU64) -> ChangeRequestCheckout {
        ChangeRequestCheckout {
            forge: None,
            number,
            project_path: None,
        }
    }

    pub fn number(&self) -> NonZeroU64 {
        self.number
    }

    pub fn forge(&self) -> Option<&str> {
        self.forge.as_deref()
    }

    pub fn set_forge(&mut self, forge: impl Into<String>) {
        self.forge = Some(forge.into());
    }

    pub fn with_forge(mut self, forge: impl Into<String>) -> ChangeRequestCheckout {
        self.set_forge(forge);
        self
    }

    pub fn project_path(&self) -> Option<&str> {
        self.project_path.as_deref()
    }

    pub fn set_project_path(&mut self, project_path: impl Into<String>) {
        self.project_path = Some(project_path.into());
    }

    pub fn with_project_path(mut self, project_path: impl Into<String>) -> ChangeRequestCheckout {
        self.set_project_path(project_path);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatedWorkspace {
    workspace: WorkspaceDescriptor,
    agent: Option<AgentSnapshot>,
    setup_terminal_id: Option<String>,
    setup_skipped_reason: Option<String>,
}

impl CreatedWorkspace {
    pub(crate) fn new(
        workspace: WorkspaceDescriptor,
        agent: Option<AgentSnapshot>,
        setup_terminal_id: Option<String>,
        setup_skipped_reason: Option<String>,
    ) -> CreatedWorkspace {
        CreatedWorkspace {
            workspace,
            agent,
            setup_terminal_id,
            setup_skipped_reason,
        }
    }

    pub fn workspace(&self) -> &WorkspaceDescriptor {
        &self.workspace
    }

    pub fn agent(&self) -> Option<&AgentSnapshot> {
        self.agent.as_ref()
    }

    pub fn setup_terminal_id(&self) -> Option<&str> {
        self.setup_terminal_id.as_deref()
    }

    pub fn setup_skipped_reason(&self) -> Option<&str> {
        self.setup_skipped_reason.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceCreated {
    pub(crate) workspace: Option<WorkspaceDescriptor>,
    #[serde(default)]
    pub(crate) agent: Option<AgentSnapshot>,
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) setup_terminal_id: Option<String>,
    #[serde(default)]
    pub(crate) setup_skipped_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSortKey {
    ActivityAt,
    Name,
    ProjectId,
    StatusPriority,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirectoryQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<WorkspaceFilter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sort: Vec<Sort<WorkspaceSortKey>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<Page>,
}

impl WorkspaceDirectoryQuery {
    pub fn search(&self) -> Option<&str> {
        self.filter.as_ref()?.query.as_deref()
    }

    pub fn set_search(&mut self, query: impl Into<String>) {
        self.filter
            .get_or_insert_with(WorkspaceFilter::default)
            .query = Some(query.into());
    }

    pub fn with_search(mut self, query: impl Into<String>) -> WorkspaceDirectoryQuery {
        self.set_search(query);
        self
    }

    pub fn project_id(&self) -> Option<&str> {
        self.filter.as_ref()?.project_id.as_deref()
    }

    pub fn set_project_id(&mut self, project_id: impl Into<String>) {
        self.filter
            .get_or_insert_with(WorkspaceFilter::default)
            .project_id = Some(project_id.into());
    }

    pub fn with_project_id(mut self, project_id: impl Into<String>) -> WorkspaceDirectoryQuery {
        self.set_project_id(project_id);
        self
    }

    pub fn add_sort(&mut self, key: WorkspaceSortKey, direction: SortDirection) {
        self.sort.push(Sort { key, direction });
    }

    pub fn with_sort(
        mut self,
        key: WorkspaceSortKey,
        direction: SortDirection,
    ) -> WorkspaceDirectoryQuery {
        self.add_sort(key, direction);
        self
    }

    pub fn set_page(&mut self, limit: u32, cursor: Option<String>) -> Result<(), ArgumentError> {
        self.page = Some(Page::new(limit, cursor)?);
        Ok(())
    }

    pub fn with_page(
        mut self,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<WorkspaceDirectoryQuery, ArgumentError> {
        self.set_page(limit, cursor)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Fuzzy,
    Suffix,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySuggestionsQuery {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    include_files: bool,
    include_directories: bool,
    match_mode: MatchMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

impl DirectorySuggestionsQuery {
    pub fn directories(query: impl Into<String>) -> DirectorySuggestionsQuery {
        DirectorySuggestionsQuery {
            query: query.into(),
            cwd: None,
            include_files: false,
            include_directories: true,
            match_mode: MatchMode::Fuzzy,
            limit: None,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> DirectorySuggestionsQuery {
        self.set_cwd(cwd);
        self
    }

    pub fn include_files(&self) -> bool {
        self.include_files
    }

    pub fn set_include_files(&mut self, include_files: bool) {
        self.include_files = include_files;
    }

    pub fn with_include_files(mut self, include_files: bool) -> DirectorySuggestionsQuery {
        self.set_include_files(include_files);
        self
    }

    pub fn include_directories(&self) -> bool {
        self.include_directories
    }

    pub fn set_include_directories(&mut self, include_directories: bool) {
        self.include_directories = include_directories;
    }

    pub fn with_include_directories(
        mut self,
        include_directories: bool,
    ) -> DirectorySuggestionsQuery {
        self.set_include_directories(include_directories);
        self
    }

    pub fn match_mode(&self) -> MatchMode {
        self.match_mode
    }

    pub fn set_match_mode(&mut self, match_mode: MatchMode) {
        self.match_mode = match_mode;
    }

    pub fn with_match_mode(mut self, match_mode: MatchMode) -> DirectorySuggestionsQuery {
        self.set_match_mode(match_mode);
        self
    }

    pub fn limit(&self) -> Option<u32> {
        self.limit
    }

    pub fn set_limit(&mut self, limit: u32) -> Result<(), ArgumentError> {
        const MAX_LIMIT: u32 = 100;
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ArgumentError::LimitOutOfRange {
                name: "directory suggestion limit",
                value: limit,
                max: MAX_LIMIT,
            });
        }
        self.limit = Some(limit);
        Ok(())
    }

    pub fn with_limit(mut self, limit: u32) -> Result<DirectorySuggestionsQuery, ArgumentError> {
        self.set_limit(limit)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Directory,
    File,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DirectorySuggestion {
    path: String,
    kind: EntryKind,
}

impl DirectorySuggestion {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkspaceRecovery {
    Recoverable {
        workspace_id: String,
        workspace_name: String,
        action: String,
        branch: Option<String>,
    },
    Unavailable {
        workspace_id: String,
        reason: String,
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionCleared {
    workspace_id: String,
    cleared_agent_ids: Vec<String>,
    success: bool,
    error: Option<String>,
}

impl AttentionCleared {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn cleared_agent_ids(&self) -> &[String] {
        &self.cleared_agent_ids
    }

    pub fn success(&self) -> bool {
        self.success
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClosedAgent {
    agent_id: String,
    archived_at: String,
}

impl ClosedAgent {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn archived_at(&self) -> &str {
        &self.archived_at
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClosedTerminal {
    terminal_id: String,
    success: bool,
}

impl ClosedTerminal {
    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }

    pub fn success(&self) -> bool {
        self.success
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ClosedItems {
    agents: Vec<ClosedAgent>,
    terminals: Vec<ClosedTerminal>,
}

impl ClosedItems {
    pub fn agents(&self) -> &[ClosedAgent] {
        &self.agents
    }

    pub fn terminals(&self) -> &[ClosedTerminal] {
        &self.terminals
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectIcon {
    mime_type: String,
    bytes: Vec<u8>,
}

impl ProjectIcon {
    pub(crate) fn new(mime_type: String, bytes: Vec<u8>) -> ProjectIcon {
        ProjectIcon { mime_type, bytes }
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
