use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitRuntime {
    #[serde(default)]
    pub current_branch: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_display_name: String,
    #[serde(default)]
    pub project_root_path: String,
    #[serde(default)]
    pub workspace_directory: Option<String>,
    #[serde(default)]
    pub workspace_kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub git_runtime: Option<WorkspaceGitRuntime>,
}

impl Workspace {
    pub fn cwd(&self) -> &str {
        self.workspace_directory
            .as_deref()
            .filter(|d| !d.is_empty())
            .unwrap_or(&self.project_root_path)
    }

    pub fn branch(&self) -> Option<&str> {
        self.git_runtime
            .as_ref()?
            .current_branch
            .as_deref()
            .filter(|b| !b.is_empty())
    }
}

pub fn fetch_workspaces_request(request_id: &str) -> Value {
    json!({ "type": "fetch_workspaces_request", "requestId": request_id })
}

pub fn project_add_request(request_id: &str, cwd: &str) -> Value {
    json!({ "type": "project.add.request", "cwd": cwd, "requestId": request_id })
}

pub fn project_create_directory_request(request_id: &str, parent_path: &str, name: &str) -> Value {
    json!({
        "type": "project.create_directory.request",
        "parentPath": parent_path,
        "name": name,
        "requestId": request_id
    })
}

pub fn directory_suggestions_request(request_id: &str, query: &str, limit: u32) -> Value {
    json!({
        "type": "directory_suggestions_request",
        "query": query,
        "includeDirectories": true,
        "includeFiles": false,
        "matchMode": "fuzzy",
        "limit": limit,
        "requestId": request_id
    })
}

pub fn branch_suggestions_request(request_id: &str, cwd: &str, query: &str, limit: u32) -> Value {
    json!({
        "type": "branch_suggestions_request",
        "cwd": cwd,
        "query": query,
        "limit": limit,
        "requestId": request_id
    })
}

pub fn workspace_create_directory_request(request_id: &str, path: &str) -> Value {
    json!({
        "type": "workspace.create.request",
        "requestId": request_id,
        "source": { "kind": "directory", "path": path }
    })
}

pub fn workspace_create_worktree_request(
    request_id: &str,
    cwd: &str,
    action: &str,
    ref_name: Option<&str>,
    base_branch: Option<&str>,
    branch_name: Option<&str>,
) -> Value {
    let mut source = json!({ "kind": "worktree", "cwd": cwd, "action": action });
    if let Some(ref_name) = ref_name {
        source["refName"] = json!(ref_name);
    }
    if let Some(base_branch) = base_branch {
        source["baseBranch"] = json!(base_branch);
    }
    if let Some(branch_name) = branch_name {
        source["branchName"] = json!(branch_name);
    }
    json!({
        "type": "workspace.create.request",
        "requestId": request_id,
        "source": source
    })
}

pub fn project_github_clone_request(request_id: &str, repo: &str, protocol: &str) -> Value {
    json!({
        "type": "project.github.clone.request",
        "repo": repo,
        "cloneProtocol": protocol,
        "requestId": request_id
    })
}
