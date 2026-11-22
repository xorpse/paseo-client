use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelColour {
    Amber,
    Blue,
    Emerald,
    Indigo,
    Orange,
    Pink,
    Red,
    Sky,
    Teal,
    Violet,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceLabel {
    name: String,
    #[serde(rename = "color")]
    colour: LabelColour,
}

impl WorkspaceLabel {
    pub fn new(name: impl Into<String>, colour: LabelColour) -> WorkspaceLabel {
        WorkspaceLabel {
            name: name.into(),
            colour,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn colour(&self) -> LabelColour {
        self.colour
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LabelSyncMode {
    Changes,
    Snapshot,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LabelRemoval {
    name: String,
    seq: u64,
}

impl LabelRemoval {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelSync {
    mode: LabelSyncMode,
    generation: String,
    head_seq: u64,
    #[serde(default)]
    removals: Vec<LabelRemoval>,
}

impl LabelSync {
    pub fn mode(&self) -> LabelSyncMode {
        self.mode
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn head_seq(&self) -> u64 {
        self.head_seq
    }

    pub fn removals(&self) -> &[LabelRemoval] {
        &self.removals
    }

    pub fn cursor(&self) -> LabelCursor {
        LabelCursor {
            generation: self.generation.clone(),
            after_seq: self.head_seq,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelCursor {
    generation: String,
    after_seq: u64,
}

impl LabelCursor {
    pub fn new(generation: impl Into<String>, after_seq: u64) -> LabelCursor {
        LabelCursor {
            generation: generation.into(),
            after_seq,
        }
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn after_seq(&self) -> u64 {
        self.after_seq
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LabelCatalogue {
    labels: Vec<WorkspaceLabel>,
    sync: LabelSync,
}

impl LabelCatalogue {
    pub fn labels(&self) -> &[WorkspaceLabel] {
        &self.labels
    }

    pub fn sync(&self) -> &LabelSync {
        &self.sync
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelUpserted {
    label: WorkspaceLabel,
    #[serde(default)]
    previous_name: Option<String>,
    generation: String,
    seq: u64,
}

impl LabelUpserted {
    pub fn label(&self) -> &WorkspaceLabel {
        &self.label
    }

    pub fn previous_name(&self) -> Option<&str> {
        self.previous_name.as_deref()
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LabelRemoved {
    name: String,
    generation: String,
    seq: u64,
}

impl LabelRemoved {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LabelUpdate {
    Remove(LabelRemoved),
    Upsert(LabelUpserted),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelAssignment {
    label: WorkspaceLabel,
    workspace_labels: Vec<String>,
}

impl LabelAssignment {
    pub fn label(&self) -> &WorkspaceLabel {
        &self.label
    }

    pub fn workspace_labels(&self) -> &[String] {
        &self.workspace_labels
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelEdit {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_name: Option<String>,
    #[serde(rename = "color", skip_serializing_if = "Option::is_none")]
    colour: Option<LabelColour>,
}

impl LabelEdit {
    pub fn new(name: impl Into<String>) -> LabelEdit {
        LabelEdit {
            name: name.into(),
            new_name: None,
            colour: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new_name(&self) -> Option<&str> {
        self.new_name.as_deref()
    }

    pub fn set_new_name(&mut self, new_name: impl Into<String>) {
        self.new_name = Some(new_name.into());
    }

    pub fn with_new_name(mut self, new_name: impl Into<String>) -> LabelEdit {
        self.set_new_name(new_name);
        self
    }

    pub fn colour(&self) -> Option<LabelColour> {
        self.colour
    }

    pub fn set_colour(&mut self, colour: LabelColour) {
        self.colour = Some(colour);
    }

    pub fn with_colour(mut self, colour: LabelColour) -> LabelEdit {
        self.set_colour(colour);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditedLabel {
    label: WorkspaceLabel,
    affected_workspace_count: u64,
}

impl EditedLabel {
    pub fn label(&self) -> &WorkspaceLabel {
        &self.label
    }

    pub fn affected_workspace_count(&self) -> u64 {
        self.affected_workspace_count
    }
}
