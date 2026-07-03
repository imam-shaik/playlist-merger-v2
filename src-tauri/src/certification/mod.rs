//! Runtime Certification Suite
//!
//! This module provides automated runtime certification for the merge pipeline.
//! It tests the actual behavior of the subtitle timeline engine with generated
//! test media.

pub mod subtitle_runtime_certification;
pub mod decision_idempotency_certification;
pub mod cross_process_certification;
pub mod execution_certification;
pub mod media_certification;
pub mod recovery_certification;
pub mod backend_certification;
pub mod stability_certification;
pub mod packet_timestamp_certification;

pub use subtitle_runtime_certification::{
    CertificationConfig, CertificationResult, CertificationTests,
    run_certification_suite, verify_timeline_integrity,
    parse_srt_timestamps, generate_test_video, generate_test_srt,
    extract_subtitles_from_video,
};

pub use decision_idempotency_certification::{
    DecisionIdempotencyReport, ComparisonResult,
    certify_decision_idempotency,
};

pub use cross_process_certification::{
    CrossProcessReport, CrossComparison,
    certify_cross_process, save_plan_for_debug,
};

pub use execution_certification::{
    ExecutionPlan, ExecutionCertReport, ExecutionComparison,
    certify_execution_idempotency, InputFilePlan, FfmpegPlan,
    ConcatListPlan, SubtitlePlan, NormalizationPlan,
};

pub use media_certification::{
    MediaSemanticReport, MediaLayerReport, MediaCheck, CheckSeverity,
    MediaFingerprint, MediaSemanticCertifier, StreamExpectation,
    MediaProbe, MediaProbeFromJson, certify_media_semantic,
};

pub use recovery_certification::{
    RecoveryFingerprint, RecoveryCertReport, RecoveryLayerReport, RecoveryCheck,
    RecoveryCertifier, CheckpointState, NormalizedFile, TempFile,
    ConcatListState, CompletedPhase, RecoveryPath,
    certify_recovery_idempotency,
};

pub use backend_certification::{
    BackendCapabilityMatrix, BackendCapabilities, ComparableFeature, NonComparableFeature,
    BackendSemanticsReport, BackendLayerReport, BackendCheck,
    BackendDifference, BackendSemanticFingerprint,
    BackendCertifier, BackendExecutionPlan,
    TimelineToleranceConfig, TIMELINE_TOLERANCE_DEFAULT,
    BackendDifferenceRegistry, RegisteredDifference,
    BackendRoutingFingerprint, BackendDiagnostics,
    BackendWarning, BackendError, BackendRepair,
    BackendSelectionDeterminismReport, BackendDiagnosticsComparison, DiagnosticEquivalence,
    compute_backend_fingerprint, certify_backend_semantic_equivalence,
    certify_backend_selection_determinism,
};

pub use stability_certification::{
    StabilityCertConfig, StabilityMetrics,
    StressTestResult, MemoryStabilityResult, HandleStabilityResult,
    ThreadStabilityResult, ParallelMergeResult, LongPlaylistResult,
    CancellationStressResult, RecoveryResult, CacheStabilityResult,
    BackendParityResult, StabilityCertificationReport,
    measure_rss_mb, measure_thread_count, measure_file_handle_count,
    check_plateau, compute_growth_rate,
};

pub use packet_timestamp_certification::{
    PacketTimestampReport, PacketTimestampLayerReport, PacketTimestampCheck,
    PtsCheckSeverity, PacketTimestampIssue, PacketTimestampAnalyzer,
    certify_packet_timestamps,
};

pub use crate::ffmpeg::normalization::MergePlan;