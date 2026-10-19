use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::protocol::terminals::TerminalSize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSpawn {
    command: String,
    args: Vec<String>,
    cwd: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pty: Option<TerminalSize>,
}

impl ProcessSpawn {
    pub fn new(command: impl Into<String>, cwd: impl Into<String>) -> ProcessSpawn {
        ProcessSpawn {
            command: command.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: HashMap::new(),
            pty: None,
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn add_arg(&mut self, arg: impl Into<String>) {
        self.args.push(arg.into());
    }

    pub fn with_arg(mut self, arg: impl Into<String>) -> ProcessSpawn {
        self.add_arg(arg);
        self
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    pub fn set_env(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.env.insert(name.into(), value.into());
    }

    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> ProcessSpawn {
        self.set_env(name, value);
        self
    }

    pub fn pty(&self) -> Option<TerminalSize> {
        self.pty
    }

    pub fn set_pty(&mut self, size: TerminalSize) {
        self.pty = Some(size);
    }

    pub fn with_pty(mut self, size: TerminalSize) -> ProcessSpawn {
        self.set_pty(size);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExit {
    exit_code: Option<i64>,
    signal: Option<String>,
}

impl ProcessExit {
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.and_then(|code| i32::try_from(code).ok())
    }

    pub fn signal(&self) -> Option<&str> {
        self.signal.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct NetClosed {
    error: Option<String>,
}

impl NetClosed {
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
