use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SkillSelection {
    All,
    Custom {
        skills: Vec<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SkillsState {
    Drift,
    NotInstalled,
    UpToDate,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillOperation {
    Add {
        name: String,
    },
    Delete {
        name: String,
    },
    Update {
        name: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SkillsStatus {
    state: SkillsState,
    ops: Vec<SkillOperation>,
    available: Vec<String>,
    installed: Vec<String>,
    selection: SkillSelection,
}

impl SkillsStatus {
    pub fn state(&self) -> SkillsState {
        self.state
    }

    pub fn operations(&self) -> &[SkillOperation] {
        &self.ops
    }

    pub fn available(&self) -> &[String] {
        &self.available
    }

    pub fn installed(&self) -> &[String] {
        &self.installed
    }

    pub fn selection(&self) -> &SkillSelection {
        &self.selection
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct Confirmation {
    removals: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSkills {
    #[serde(flatten)]
    status: SkillsStatus,
    #[serde(default)]
    confirmation_required: Option<Confirmation>,
}

impl SavedSkills {
    pub fn status(&self) -> &SkillsStatus {
        &self.status
    }

    pub fn removals_to_confirm(&self) -> Option<&[String]> {
        self.confirmation_required
            .as_ref()
            .map(|confirmation| confirmation.removals.as_slice())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ImportedSkills {
    imported: bool,
    selection: SkillSelection,
}

impl ImportedSkills {
    pub fn imported(&self) -> bool {
        self.imported
    }

    pub fn selection(&self) -> &SkillSelection {
        &self.selection
    }
}
