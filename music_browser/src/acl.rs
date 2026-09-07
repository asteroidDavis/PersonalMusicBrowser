use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    Viewer,
    Editor,
    Admin,
}

impl AccessLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            AccessLevel::Viewer => "viewer",
            AccessLevel::Editor => "editor",
            AccessLevel::Admin => "admin",
        }
    }

    pub fn can_edit(self) -> bool {
        matches!(self, AccessLevel::Editor | AccessLevel::Admin)
    }

    pub fn can_admin(self) -> bool {
        matches!(self, AccessLevel::Admin)
    }
}

impl fmt::Display for AccessLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Instrument,
    Band,
    Artist,
    Album,
    Song,
    Recording,
    Device,
    DevicePreset,
    SongInstrument,
    ProductionStage,
    ProductionStep,
    SongFile,
    Sample,
    PracticeExercise,
    Goal,
    LiveSet,
    Job,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceType::Instrument => "instrument",
            ResourceType::Band => "band",
            ResourceType::Artist => "artist",
            ResourceType::Album => "album",
            ResourceType::Song => "song",
            ResourceType::Recording => "recording",
            ResourceType::Device => "device",
            ResourceType::DevicePreset => "device_preset",
            ResourceType::SongInstrument => "song_instrument",
            ResourceType::ProductionStage => "production_stage",
            ResourceType::ProductionStep => "production_step",
            ResourceType::SongFile => "song_file",
            ResourceType::Sample => "sample",
            ResourceType::PracticeExercise => "practice_exercise",
            ResourceType::Goal => "goal",
            ResourceType::LiveSet => "live_set",
            ResourceType::Job => "job",
        }
    }

    /// Parse the `as_str` form back into a `ResourceType`.
    pub fn parse(s: &str) -> Option<Self> {
        [
            ResourceType::Instrument,
            ResourceType::Band,
            ResourceType::Artist,
            ResourceType::Album,
            ResourceType::Song,
            ResourceType::Recording,
            ResourceType::Device,
            ResourceType::DevicePreset,
            ResourceType::SongInstrument,
            ResourceType::ProductionStage,
            ResourceType::ProductionStep,
            ResourceType::SongFile,
            ResourceType::Sample,
            ResourceType::PracticeExercise,
            ResourceType::Goal,
            ResourceType::LiveSet,
            ResourceType::Job,
        ]
        .into_iter()
        .find(|rt| rt.as_str() == s)
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Share {
    pub id: String,
    pub user_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub access_level: AccessLevel,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateShare {
    pub user_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub access_level: AccessLevel,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateGroup {
    pub name: String,
    pub description: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl GroupRole {
    pub fn as_str(self) -> &'static str {
        match self {
            GroupRole::Owner => "owner",
            GroupRole::Admin => "admin",
            GroupRole::Member => "member",
            GroupRole::Viewer => "viewer",
        }
    }

    /// Whether the role may manage the group (members, shares).
    pub fn can_manage(self) -> bool {
        matches!(self, GroupRole::Owner | GroupRole::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMember {
    pub id: String,
    pub group_id: String,
    pub user_id: String,
    pub role: GroupRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateGroupMember {
    pub group_id: String,
    pub user_id: String,
    pub role: GroupRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateGroupShare {
    pub group_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub access_level: AccessLevel,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupShare {
    pub id: String,
    pub group_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub access_level: AccessLevel,
    pub created_by: String,
}
