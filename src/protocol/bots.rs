use serde::{Deserialize, Serialize};

use crate::protocol::schedules::{Schedule, ScheduleCadence};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BotJobStatus {
    Cancelled,
    Failed,
    Finished,
    Interrupted,
    Queued,
    Running,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotDraft {
    name: String,
    instructions: String,
    provider: String,
    model: Option<String>,
    mode_id: Option<String>,
    project_path: String,
}
impl BotDraft {
    pub fn new(
        name: String,
        instructions: String,
        provider: String,
        model: Option<String>,
        mode_id: Option<String>,
        project_path: String,
    ) -> BotDraft {
        BotDraft {
            name,
            instructions,
            provider,
            model,
            mode_id,
            project_path,
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
    pub fn provider(&self) -> &str {
        &self.provider
    }
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }
    pub fn project_path(&self) -> &str {
        &self.project_path
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotJob {
    id: String,
    prompt: String,
    status: BotJobStatus,
    agent_id: String,
    workspace_id: Option<String>,
    created_at: String,
    ended_at: Option<String>,
    output: Option<String>,
    error: Option<String>,
    source: String,
}
impl BotJob {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn prompt(&self) -> &str {
        &self.prompt
    }
    pub fn status(&self) -> BotJobStatus {
        self.status
    }
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn ended_at(&self) -> Option<&str> {
        self.ended_at.as_deref()
    }
    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bot {
    id: String,
    name: String,
    instructions: String,
    provider: String,
    model: Option<String>,
    mode_id: Option<String>,
    project_path: String,
    revision: u64,
    paused: bool,
    created_at: String,
    jobs: Vec<BotJob>,
}
impl Bot {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
    pub fn provider(&self) -> &str {
        &self.provider
    }
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }
    pub fn project_path(&self) -> &str {
        &self.project_path
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn jobs(&self) -> &[BotJob] {
        &self.jobs
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotInspection {
    bot: Bot,
    memory: String,
    home: String,
    total_jobs: u64,
    schedules: Vec<Schedule>,
}
impl BotInspection {
    pub fn bot(&self) -> &Bot {
        &self.bot
    }
    pub fn memory(&self) -> &str {
        &self.memory
    }
    pub fn schedules(&self) -> &[Schedule] {
        &self.schedules
    }
    pub fn total_jobs(&self) -> u64 {
        self.total_jobs
    }
    pub fn home(&self) -> &str {
        &self.home
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotReply {
    bots: Vec<Bot>,
    inspection: Option<BotInspection>,
}
impl BotReply {
    pub fn bots(&self) -> &[Bot] {
        &self.bots
    }
    pub fn inspection(&self) -> Option<&BotInspection> {
        self.inspection.as_ref()
    }
}

#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BotAction {
    Cancel {
        bot_id: String,
        job_id: String,
    },
    Create {
        bot_id: String,
        draft: BotDraft,
    },
    Inspect {
        bot_id: String,
        offset: u32,
    },
    List,
    Pause {
        bot_id: String,
        paused: bool,
    },
    Resume {
        bot_id: String,
        job_id: String,
    },
    Save {
        bot_id: String,
        revision: u64,
        draft: BotDraft,
        memory: String,
        expected_memory: String,
    },
    Schedule {
        bot_id: String,
        name: String,
        prompt: String,
        cadence: ScheduleCadence,
    },
    ScheduleDelete {
        bot_id: String,
        schedule_id: String,
    },
    SchedulePause {
        bot_id: String,
        schedule_id: String,
        paused: bool,
    },
    Submit {
        bot_id: String,
        job_id: String,
        prompt: String,
    },
}
