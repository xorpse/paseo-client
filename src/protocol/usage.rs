use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageList {
    #[serde(default)]
    fetched_at: Option<String>,
    #[serde(default)]
    providers: Vec<ProviderUsage>,
}

impl ProviderUsageList {
    pub fn fetched_at(&self) -> Option<&str> {
        self.fetched_at.as_deref()
    }

    pub fn providers(&self) -> &[ProviderUsage] {
        &self.providers
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUsageStatus {
    Available,
    Error,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    provider_id: String,
    #[serde(default)]
    display_name: Option<String>,
    status: ProviderUsageStatus,
    #[serde(default)]
    plan_label: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    windows: Vec<UsageWindow>,
    #[serde(default)]
    balances: Vec<UsageBalance>,
}

impl ProviderUsage {
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn status(&self) -> ProviderUsageStatus {
        self.status
    }

    pub fn plan_label(&self) -> Option<&str> {
        self.plan_label.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn windows(&self) -> &[UsageWindow] {
        &self.windows
    }

    pub fn balances(&self) -> &[UsageBalance] {
        &self.balances
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UsageTone {
    Danger,
    Default,
    Ok,
    Warning,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    id: String,
    label: String,
    #[serde(default)]
    short_label: Option<String>,
    #[serde(default)]
    summary: bool,
    #[serde(default)]
    used_pct: Option<f64>,
    #[serde(default)]
    remaining_pct: Option<f64>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    runs_out_at: Option<String>,
    #[serde(default)]
    shortfall_pct: Option<f64>,
    #[serde(default)]
    tone: Option<UsageTone>,
}

impl UsageWindow {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn short_label(&self) -> Option<&str> {
        self.short_label.as_deref()
    }

    /// Whether the window belongs in the usage summary until the user picks windows of their own.
    pub fn summary(&self) -> bool {
        self.summary
    }

    pub fn used_pct(&self) -> Option<f64> {
        self.used_pct
    }

    pub fn remaining_pct(&self) -> Option<f64> {
        self.remaining_pct
    }

    pub fn resets_at(&self) -> Option<&str> {
        self.resets_at.as_deref()
    }

    /// When the window is forecast to run out before it resets.
    pub fn runs_out_at(&self) -> Option<&str> {
        self.runs_out_at.as_deref()
    }

    /// How far short of the reset the forecast runs out, as a percentage of the window.
    pub fn shortfall_pct(&self) -> Option<f64> {
        self.shortfall_pct
    }

    pub fn tone(&self) -> Option<UsageTone> {
        self.tone
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageBalance {
    id: String,
    label: String,
    #[serde(default)]
    used: Option<f64>,
    #[serde(default)]
    remaining: Option<f64>,
    #[serde(default)]
    limit: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    tone: Option<UsageTone>,
}

impl UsageBalance {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn used(&self) -> Option<f64> {
        self.used
    }

    pub fn remaining(&self) -> Option<f64> {
        self.remaining
    }

    pub fn limit(&self) -> Option<f64> {
        self.limit
    }

    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    pub fn resets_at(&self) -> Option<&str> {
        self.resets_at.as_deref()
    }

    pub fn tone(&self) -> Option<UsageTone> {
        self.tone
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UsageDetail {
    id: String,
    label: String,
    value: String,
}

impl UsageDetail {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUsage {
    #[serde(default)]
    plan_label: Option<String>,
    windows: Vec<UsageWindow>,
    #[serde(default)]
    balances: Vec<UsageBalance>,
    #[serde(default)]
    details: Vec<UsageDetail>,
}

impl AvailableUsage {
    pub fn plan_label(&self) -> Option<&str> {
        self.plan_label.as_deref()
    }

    pub fn windows(&self) -> &[UsageWindow] {
        &self.windows
    }

    pub fn balances(&self) -> &[UsageBalance] {
        &self.balances
    }

    pub fn details(&self) -> &[UsageDetail] {
        &self.details
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum UsageProblem {
    Expired {
        expires_at: String,
        #[serde(default)]
        refreshed_by: Option<String>,
    },
    NoQuota {
        detail: String,
    },
    Rejected {
        status: u16,
        #[serde(default)]
        refreshed_by: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UsageReport {
    Available(AvailableUsage),
    Error {
        error: String,
    },
    Unavailable {
        problem: UsageProblem,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LastSuccessfulUsage {
    fetched_at: String,
    report: AvailableUsage,
}

impl LastSuccessfulUsage {
    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    pub fn report(&self) -> &AvailableUsage {
        &self.report
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UsageLoginError {
    harness: String,
    report: UsageReport,
}

impl UsageLoginError {
    pub fn harness(&self) -> &str {
        &self.harness
    }

    pub fn report(&self) -> &UsageReport {
        &self.report
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct UsageAccount {
    #[serde(default)]
    label: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageReportEntry {
    id: String,
    #[serde(default)]
    account: UsageAccount,
    fetched_at: String,
    source_id: String,
    source_label: String,
    #[serde(default)]
    icon: Option<String>,
    report: UsageReport,
    #[serde(default)]
    successful_at: Option<String>,
    #[serde(default)]
    last_successful: Option<LastSuccessfulUsage>,
    #[serde(default)]
    login_errors: Vec<UsageLoginError>,
}

impl UsageReportEntry {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn account_label(&self) -> Option<&str> {
        self.account.label.as_deref()
    }

    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn report(&self) -> &UsageReport {
        &self.report
    }

    pub fn successful_at(&self) -> Option<&str> {
        self.successful_at.as_deref()
    }

    /// The last report that succeeded, kept while the current one is unavailable.
    pub fn last_successful(&self) -> Option<&LastSuccessfulUsage> {
        self.last_successful.as_ref()
    }

    pub fn login_errors(&self) -> &[UsageLoginError] {
        &self.login_errors
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReportsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force_refresh: Option<bool>,
}

impl UsageReportsQuery {
    pub fn all() -> UsageReportsQuery {
        UsageReportsQuery::default()
    }

    pub fn for_agent(agent_id: impl Into<String>) -> UsageReportsQuery {
        UsageReportsQuery {
            agent_id: Some(agent_id.into()),
            ..UsageReportsQuery::default()
        }
    }

    pub fn for_reports(report_ids: Vec<String>) -> UsageReportsQuery {
        UsageReportsQuery {
            report_ids: Some(report_ids),
            ..UsageReportsQuery::default()
        }
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    pub fn report_ids(&self) -> Option<&[String]> {
        self.report_ids.as_deref()
    }

    pub fn force_refresh(&self) -> bool {
        self.force_refresh == Some(true)
    }

    /// Fetches every report afresh instead of serving the daemon's cache.
    pub fn set_force_refresh(&mut self, force_refresh: bool) {
        self.force_refresh = Some(force_refresh);
    }

    pub fn with_force_refresh(mut self, force_refresh: bool) -> UsageReportsQuery {
        self.set_force_refresh(force_refresh);
        self
    }
}
