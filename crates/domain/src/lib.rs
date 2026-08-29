use thiserror::Error;

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

pub fn validate_contest_end_extension(
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
    /// Final verdicts and cancellation end the judging lifecycle. Rejudging
    /// deliberately bypasses this machine: an operator resets a submission
    /// from any state back to `Pending` and supersedes its judgement, which is
    /// an administrative action rather than a judged transition.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Judging)
    }

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
        ContestExtensionError, ContestSchedule, ContestState, ContestTransitionError,
        SubmissionState, validate_contest_end_extension, validate_contest_transition,
    };
    use time::{Duration, OffsetDateTime};

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
            validate_contest_end_extension(
                ContestState::Running,
                Some(end),
                end - Duration::SECOND,
                end + Duration::HOUR,
            ),
            Err(ContestExtensionError::Stale)
        );
        assert_eq!(
            validate_contest_end_extension(
                ContestState::Draft,
                Some(end),
                end,
                end + Duration::HOUR,
            ),
            Err(ContestExtensionError::InvalidState(ContestState::Draft))
        );
        assert_eq!(
            validate_contest_end_extension(
                ContestState::Paused,
                Some(end),
                end,
                end + Duration::HOUR,
            ),
            Ok(end)
        );
    }
}
