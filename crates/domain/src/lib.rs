use std::fmt;

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContestId(i64);

impl TryFrom<i64> for ContestId {
    type Error = InvalidId;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value <= 0 {
            return Err(InvalidId(value));
        }
        Ok(Self(value))
    }
}

impl From<ContestId> for i64 {
    fn from(value: ContestId) -> Self {
        value.0
    }
}

impl fmt::Display for ContestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JudgementId(Uuid);

impl JudgementId {
    #[must_use]
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn into_inner(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionState {
    Pending,
    Judging,
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    CompileError,
    OutputLimitExceeded,
    SystemError,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContestState {
    Draft,
    FrozenConfig,
    Running,
    Paused,
    Ended,
    Archived,
}

impl ContestState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::FrozenConfig)
                | (Self::FrozenConfig, Self::Running)
                | (Self::Running, Self::Paused | Self::Ended)
                | (Self::Paused, Self::Running)
                | (Self::Ended, Self::Archived)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestSchedule {
    pub start_at: time::OffsetDateTime,
    pub freeze_at: time::OffsetDateTime,
    pub end_at: time::OffsetDateTime,
}

impl ContestSchedule {
    pub fn validate(self) -> Result<Self, ContestScheduleError> {
        if self.start_at > self.freeze_at {
            return Err(ContestScheduleError::StartAfterFreeze);
        }
        if self.freeze_at > self.end_at {
            return Err(ContestScheduleError::FreezeAfterEnd);
        }
        Ok(self)
    }
}

pub fn validate_contest_transition(
    from: ContestState,
    to: ContestState,
    schedule: Option<ContestSchedule>,
) -> Result<(), ContestTransitionError> {
    if !from.can_transition_to(to) {
        return Err(ContestTransitionError::Invalid { from, to });
    }
    if to == ContestState::FrozenConfig {
        schedule
            .ok_or(ContestTransitionError::ScheduleRequired)?
            .validate()
            .map_err(ContestTransitionError::InvalidSchedule)?;
    }
    Ok(())
}

pub fn extend_contest_end(
    state: ContestState,
    current_end: Option<time::OffsetDateTime>,
    expected_end: time::OffsetDateTime,
    new_end: time::OffsetDateTime,
) -> Result<time::OffsetDateTime, ContestExtensionError> {
    if !matches!(state, ContestState::Running | ContestState::Paused) {
        return Err(ContestExtensionError::InvalidState(state));
    }
    let current_end = current_end.ok_or(ContestExtensionError::EndTimeNotSet)?;
    if current_end != expected_end {
        return Err(ContestExtensionError::Stale);
    }
    if new_end <= current_end {
        return Err(ContestExtensionError::NotLater);
    }
    Ok(current_end)
}

impl SubmissionState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        use SubmissionState::{
            Accepted, Cancelled, CompileError, Judging, MemoryLimitExceeded, OutputLimitExceeded,
            Pending, RuntimeError, SystemError, TimeLimitExceeded, WrongAnswer,
        };

        match self {
            Pending => matches!(next, Judging | Cancelled),
            Judging => matches!(
                next,
                Accepted
                    | WrongAnswer
                    | TimeLimitExceeded
                    | MemoryLimitExceeded
                    | RuntimeError
                    | CompileError
                    | OutputLimitExceeded
                    | SystemError
                    | Cancelled
            ),
            Accepted | WrongAnswer | TimeLimitExceeded | MemoryLimitExceeded | RuntimeError
            | CompileError | OutputLimitExceeded | SystemError | Cancelled => false,
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("ID must be positive, got {0}")]
pub struct InvalidId(i64);

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ContestScheduleError {
    #[error("contest start time is after freeze time")]
    StartAfterFreeze,
    #[error("contest freeze time is after end time")]
    FreezeAfterEnd,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ContestTransitionError {
    #[error("invalid contest transition from {from:?} to {to:?}")]
    Invalid { from: ContestState, to: ContestState },
    #[error("contest schedule is required before freezing configuration")]
    ScheduleRequired,
    #[error("contest schedule is invalid: {0}")]
    InvalidSchedule(ContestScheduleError),
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ContestExtensionError {
    #[error("contest cannot be extended in state {0:?}")]
    InvalidState(ContestState),
    #[error("contest end time is not set")]
    EndTimeNotSet,
    #[error("contest end time changed")]
    Stale,
    #[error("new contest end time must be later")]
    NotLater,
}

#[cfg(test)]
mod tests {
    use super::{
        ContestExtensionError, ContestId, ContestSchedule, ContestState, ContestTransitionError,
        SubmissionState, extend_contest_end, validate_contest_transition,
    };
    use time::{Duration, OffsetDateTime};

    #[test]
    fn contest_id_rejects_non_positive_values() {
        assert!(ContestId::try_from(0).is_err());
        assert!(ContestId::try_from(-1).is_err());
        assert_eq!(i64::from(ContestId::try_from(1).expect("positive ID")), 1);
    }

    #[test]
    fn terminal_submission_state_cannot_transition() {
        assert!(SubmissionState::Pending.can_transition_to(SubmissionState::Judging));
        assert!(SubmissionState::Judging.can_transition_to(SubmissionState::Accepted));
        assert!(!SubmissionState::Accepted.can_transition_to(SubmissionState::Judging));
    }

    #[test]
    fn contest_lifecycle_has_only_reviewed_edges() {
        assert!(ContestState::Draft.can_transition_to(ContestState::FrozenConfig));
        assert!(ContestState::Running.can_transition_to(ContestState::Paused));
        assert!(ContestState::Running.can_transition_to(ContestState::Ended));
        assert!(ContestState::Paused.can_transition_to(ContestState::Running));
        assert!(!ContestState::Draft.can_transition_to(ContestState::Running));
        assert!(!ContestState::Archived.can_transition_to(ContestState::Draft));
    }

    #[test]
    fn freezing_configuration_requires_a_valid_schedule() {
        assert_eq!(
            validate_contest_transition(ContestState::Draft, ContestState::FrozenConfig, None,),
            Err(ContestTransitionError::ScheduleRequired)
        );
        let start = OffsetDateTime::UNIX_EPOCH;
        let schedule = ContestSchedule {
            start_at: start,
            freeze_at: start + Duration::HOUR,
            end_at: start + Duration::HOUR * 2,
        };
        assert!(
            validate_contest_transition(
                ContestState::Draft,
                ContestState::FrozenConfig,
                Some(schedule),
            )
            .is_ok()
        );
    }

    #[test]
    fn extension_uses_expected_end_as_concurrency_guard() {
        let end = OffsetDateTime::UNIX_EPOCH + Duration::HOUR;
        assert_eq!(
            extend_contest_end(
                ContestState::Running,
                Some(end),
                end - Duration::SECOND,
                end + Duration::HOUR,
            ),
            Err(ContestExtensionError::Stale)
        );
        assert_eq!(
            extend_contest_end(ContestState::Draft, Some(end), end, end + Duration::HOUR,),
            Err(ContestExtensionError::InvalidState(ContestState::Draft))
        );
        assert_eq!(
            extend_contest_end(ContestState::Paused, Some(end), end, end + Duration::HOUR,),
            Ok(end)
        );
    }
}
