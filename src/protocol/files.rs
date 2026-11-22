use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::protocol::number::WireNumber;
use crate::protocol::workspaces::EntryKind;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    path: String,
    kind: EntryKind,
    size: WireNumber,
    modified_at: String,
}

impl FileEntry {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn size(&self) -> u64 {
        self.size.count()
    }

    pub fn modified_at(&self) -> &str {
        &self.modified_at
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct DirectoryListing {
    path: String,
    entries: Vec<FileEntry>,
}

impl DirectoryListing {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Binary,
    Image,
    Text,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContent {
    path: String,
    kind: FileKind,
    mime_type: Option<String>,
    size: u64,
    revision: FileRevision,
    bytes: Vec<u8>,
}

impl FileContent {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn kind(&self) -> FileKind {
        self.kind
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn revision(&self) -> &FileRevision {
        &self.revision
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn streamed(path: &str, metadata: TransferMetadata, bytes: Vec<u8>) -> FileContent {
        let kind = if metadata.mime.starts_with("image/") {
            FileKind::Image
        } else if metadata.encoding == TransferEncoding::Utf8
            || metadata.mime.starts_with("text/")
            || metadata.mime == "application/json"
        {
            FileKind::Text
        } else {
            FileKind::Binary
        };
        FileContent {
            path: path.to_owned(),
            kind,
            mime_type: Some(metadata.mime),
            size: metadata.size,
            revision: FileRevision {
                modified_at: metadata.modified_at,
                revision: metadata.revision,
            },
            bytes,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(crate) enum TransferEncoding {
    #[serde(rename = "binary")]
    Binary,
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferMetadata {
    mime: String,
    size: u64,
    encoding: TransferEncoding,
    modified_at: String,
    #[serde(default)]
    revision: Option<String>,
}

impl TransferMetadata {
    pub(crate) fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentEncoding {
    Base64,
    None,
    #[serde(rename = "utf-8")]
    Utf8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExplorerFile {
    path: String,
    kind: FileKind,
    encoding: ContentEncoding,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    size: WireNumber,
    modified_at: String,
    #[serde(default)]
    revision: Option<String>,
}

impl ExplorerFile {
    pub(crate) fn into_content(self) -> Result<FileContent, base64::DecodeError> {
        let content = self.content.unwrap_or_default();
        let bytes = match self.encoding {
            ContentEncoding::Base64 => STANDARD.decode(content)?,
            ContentEncoding::None => Vec::new(),
            ContentEncoding::Utf8 => content.into_bytes(),
        };
        Ok(FileContent {
            path: self.path,
            kind: self.kind,
            mime_type: self.mime_type,
            size: self.size.count(),
            revision: FileRevision {
                modified_at: self.modified_at,
                revision: self.revision,
            },
            bytes,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileRevision {
    modified_at: String,
    #[serde(default)]
    revision: Option<String>,
}

impl FileRevision {
    pub fn new(modified_at: impl Into<String>, revision: Option<String>) -> FileRevision {
        FileRevision {
            modified_at: modified_at.into(),
            revision,
        }
    }

    pub fn modified_at(&self) -> &str {
        &self.modified_at
    }

    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadyFile {
    size: WireNumber,
    #[serde(flatten)]
    revision: FileRevision,
    #[serde(default)]
    kind: Option<FileKind>,
    #[serde(default)]
    mime_type: Option<String>,
}

impl ReadyFile {
    pub fn size(&self) -> u64 {
        self.size.count()
    }

    pub fn revision(&self) -> &FileRevision {
        &self.revision
    }

    pub fn kind(&self) -> Option<FileKind> {
        self.kind
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FileState {
    Error {
        error: String,
    },
    Missing,
    Ready(ReadyFile),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FileVersion {
    cwd: String,
    path: String,
    #[serde(flatten)]
    state: FileState,
}

impl FileVersion {
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn state(&self) -> &FileState {
        &self.state
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FileWriteOutcome {
    Conflict {
        version: FileVersion,
    },
    Error {
        error: String,
    },
    Written(ReadyFile),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWrite {
    path: String,
    content: String,
    content_encoding: ContentEncoding,
    expected_modified_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_revision: Option<String>,
}

impl FileWrite {
    pub fn text(
        path: impl Into<String>,
        text: impl Into<String>,
        expected: &FileRevision,
    ) -> FileWrite {
        FileWrite::new(
            path.into(),
            text.into(),
            ContentEncoding::Utf8,
            expected.clone(),
        )
    }

    pub fn bytes(path: impl Into<String>, bytes: &[u8], expected: &FileRevision) -> FileWrite {
        FileWrite::new(
            path.into(),
            STANDARD.encode(bytes),
            ContentEncoding::Base64,
            expected.clone(),
        )
    }

    fn new(
        path: String,
        content: String,
        content_encoding: ContentEncoding,
        expected: FileRevision,
    ) -> FileWrite {
        FileWrite {
            path,
            content,
            content_encoding,
            expected_modified_at: expected.modified_at,
            expected_revision: expected.revision,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn content_encoding(&self) -> ContentEncoding {
        self.content_encoding
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadToken {
    token: String,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    size: Option<WireNumber>,
}

impl DownloadToken {
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn size(&self) -> Option<u64> {
        self.size.map(WireNumber::count)
    }
}
