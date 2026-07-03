#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DamageClassification {
    Healthy,
    TimestampDamage,
    ContainerDamage,
    SubtitleDamage,
    NeedsReencode,
    VideoDecodeFailure,
    BitstreamCorruption,
    PacketCorruption,
    VideoFrameCorruption,
    AttachmentDamage,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FileDisposition {
    Healthy,
    NeedsNormalization,
    Repairable(DamageClassification),
    Unrepairable(DamageClassification),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RepairStatus {
    Skipped,
    Succeeded,
    Failed,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RevalidationStatus {
    NotNeeded,
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FixType {
    None,
    TimestampRepair,
    ContainerRemux,
    FullReencode,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ValidationStatus {
    Clean,
    Quarantined,
    RepairedRemux,
    RepairedReencode,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}