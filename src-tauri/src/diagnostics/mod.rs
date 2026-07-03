pub mod failure_window;
pub mod normalization_delta;
pub mod container_transition;
pub mod boundary_transition;
pub mod playlist_equivalence;
pub mod runtime_timeline;
pub mod failure_preservation;
pub mod backend_comparator;
pub mod file_isolator;

pub use failure_window::{FailureWindow, ConcatPosition, SuspectFile, SuspectReason, ConfidenceLevel};
pub use normalization_delta::{
    NormalizationDelta, StreamCountDelta, StreamDelta, StreamType, StringDelta, FloatDelta,
    FormatChange, ChangeSeverity, DeltaVerdict,
};
pub use container_transition::{
    ContainerTransitionAudit, TransitionStage, StreamCounts, TimestampInfo, PtsGap, GapSeverity,
    TimestampIssue, IssueType, IssueSeverity, TimestampEvolution, TransitionVerdict,
};
pub use boundary_transition::{
    BoundaryCertification, BoundaryReport, BoundaryTransition, ConcatListIntegrity,
    ConcatListEntry, PostConcatDemuxerProbe,
    certify_boundaries, check_concat_list_integrity, probe_post_concat_demuxer,
};
pub use playlist_equivalence::{
    PlaylistEquivalenceReport, StreamProperties, EquivalenceMismatch,
    certify_playlist_equivalence, compare_extradata_hash,
};
pub use runtime_timeline::{
    RuntimeTimeline, RuntimeEvent, RuntimeTimelineRecorder, RuntimeTimelineLogger,
    FileCompletion, Milestone, CumulativeTimestampDrift, PlaylistDriftReport,
    compute_playlist_drift,
};
pub use failure_preservation::{
    FailureEvidence, EvidencePreservationConfig, EvidencePreserver, LogEntry,
};
pub use backend_comparator::{
    BackendComparisonResult, BackendRunResult, BackendComparison, BackendComparator,
};
pub use file_isolator::{
    BinaryIsolationReport, BinaryFileIsolator, SplitTestResult, IsolationIteration,
    FileIntegrityProbe, ProbeResult, scan_playlist_for_anomalies,
};

pub fn run_all_diagnostics(
    ffprobe_path: &std::path::Path,
    file_path: &std::path::Path,
    concat_list_content: &str,
    ffmpeg_stderr: &str,
    failure_time: &str,
) -> DiagnosticBundle {
    let failure_window = failure_window::parse_failure_window(
        ffmpeg_stderr,
        concat_list_content,
        failure_time,
    );

    let container_audit = container_transition::audit_container_transition(
        ffprobe_path,
        file_path,
    ).ok();

    DiagnosticBundle {
        failure_window,
        container_audit,
        normalization_delta: None,
    }
}

#[derive(Debug)]
pub struct DiagnosticBundle {
    pub failure_window: failure_window::FailureWindow,
    pub container_audit: Option<container_transition::ContainerTransitionAudit>,
    pub normalization_delta: Option<normalization_delta::NormalizationDelta>,
}

impl std::fmt::Display for DiagnosticBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.failure_window)?;
        if let Some(ref ca) = self.container_audit {
            writeln!(f, "{}", ca)?;
        }
        if let Some(ref nd) = self.normalization_delta {
            writeln!(f, "{}", nd)?;
        }
        Ok(())
    }
}

impl DiagnosticBundle {
    pub fn get_suspect_files(&self) -> Vec<&failure_window::SuspectFile> {
        self.failure_window.suspect_files.iter().collect()
    }

    pub fn has_timestamp_issues(&self) -> bool {
        self.container_audit.as_ref()
            .map(|ca| ca.transitions.iter().any(|t| t.has_invalid_timestamps))
            .unwrap_or(false)
    }

    pub fn get_first_invalid_stage(&self) -> Option<usize> {
        self.container_audit.as_ref()
            .and_then(|ca| ca.first_invalid_timestamp_stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_bundle_suspect_files() {
        let failure_window = failure_window::parse_failure_window(
            "Opening 'lecture129.mp4' for reading",
            "file '/path/lecture128.mp4'\nfile '/path/lecture129.mp4'\nfile '/path/lecture130.mp4'\n",
            "10:16:43",
        );

        let bundle = DiagnosticBundle {
            failure_window,
            container_audit: None,
            normalization_delta: None,
        };

        assert_eq!(bundle.get_suspect_files().len(), 3);
    }
}