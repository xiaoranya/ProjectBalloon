use sqlx::PgPool;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

#[derive(Debug, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityGroupResponse {
    pub problem_id: i64,
    pub language: String,
    pub fingerprint: String,
    pub submission_ids: Vec<i64>,
    pub team_ids: Vec<i64>,
    pub submission_count: i64,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityPairResponse {
    pub problem_id: i64,
    pub language: String,
    pub submission_id: i64,
    pub team_id: i64,
    pub other_submission_id: i64,
    pub other_team_id: i64,
    pub hamming_distance: i32,
    pub similarity_percent: i32,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityBackfillResponse {
    pub scanned: i64,
    pub updated: i64,
    pub failed: i64,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimilarityQuery {
    pub problem_id: Option<i64>,
    pub language: Option<String>,
    #[serde(default = "default_similarity_group_size")]
    pub min_group_size: u32,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimilarityPairQuery {
    pub problem_id: Option<i64>,
    pub language: Option<String>,
    #[serde(default = "default_similarity_percent")]
    pub min_similarity_percent: u32,
}

const fn default_similarity_group_size() -> u32 {
    2
}

const fn default_similarity_percent() -> u32 {
    85
}

impl SimilarityQuery {
    fn validate(self) -> Result<(Option<i64>, Option<String>, i64), AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if !(2..=100).contains(&self.min_group_size) {
            return Err(AppError::validation(
                "minGroupSize",
                "must contain a value between 2 and 100",
            ));
        }
        let language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if language.as_ref().is_some_and(|value| {
            !matches!(value.as_str(), "c" | "cpp" | "java" | "go" | "rust" | "python")
        }) {
            return Err(AppError::validation(
                "language",
                "must be c, cpp, java, go, rust, or python",
            ));
        }
        Ok((
            self.problem_id,
            language.filter(|value| !value.is_empty()),
            i64::from(self.min_group_size),
        ))
    }
}

impl SimilarityPairQuery {
    fn validate(self) -> Result<(Option<i64>, Option<String>, i32), AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if !(50..=100).contains(&self.min_similarity_percent) {
            return Err(AppError::validation(
                "minSimilarityPercent",
                "must contain a value between 50 and 100",
            ));
        }
        let language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if language.as_ref().is_some_and(|value| {
            !matches!(value.as_str(), "c" | "cpp" | "java" | "go" | "rust" | "python")
        }) {
            return Err(AppError::validation(
                "language",
                "must be c, cpp, java, go, rust, or python",
            ));
        }
        let min_similarity_percent = i32::try_from(self.min_similarity_percent).unwrap_or(100);
        Ok((self.problem_id, language.filter(|value| !value.is_empty()), min_similarity_percent))
    }
}

mod detail;
mod list;
mod similarity;

fn require_team_account(actor: &AuthUser) -> Result<(), AppError> {
    if actor.user_type == UserType::Team {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "TEAM_ACCOUNT_REQUIRED",
            "Only a team account can view team submissions",
        ))
    }
}

async fn team_id_for_user(database: &PgPool, user_id: i64) -> Result<i64, AppError> {
    sqlx::query_scalar("SELECT team_id FROM team_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("load submission team identity", error))?
        .ok_or_else(submission_not_found)
}

pub(super) async fn require_admin_access(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if contest_id <= 0 {
        return Err(submission_not_found());
    }
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check submission contest", error))?;
    if !active {
        return Err(submission_not_found());
    }
    if actor.is_super_admin() {
        return Ok(());
    }
    if !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE) {
        return Err(submission_not_found());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE contest_id = $1 AND user_id = $2)",
    )
    .bind(contest_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check submission contest staff", error))?;
    if assigned { Ok(()) } else { Err(submission_not_found()) }
}

fn submission_not_found() -> AppError {
    AppError::not_found("SUBMISSION_NOT_FOUND", "Submission was not found")
}

#[cfg(test)]
pub(crate) fn restricted_submission_summary() -> SubmissionSummary {
    use time::OffsetDateTime;

    SubmissionSummary {
        id: 1,
        contest_id: 7,
        problem_id: 3,
        problem_alias: "A".to_owned(),
        team_id: 2,
        team_name: "Team".to_owned(),
        language: "cpp".to_owned(),
        source_size_bytes: 128,
        status: "COMPLETED".to_owned(),
        submitted_at: OffsetDateTime::from_unix_timestamp(0).expect("epoch"),
        judged_at: None,
        active_judgement_id: None,
        verdict: Some("WRONG_ANSWER".to_owned()),
        total_time_ms: Some(12),
        peak_memory_kb: Some(2048),
        score_milli: Some(100_000),
    }
}

#[cfg(test)]
use crate::features::submissions::model::SubmissionSummary;

#[cfg(test)]
mod tests {
    use crate::features::submissions::query::{SimilarityPairQuery, SimilarityQuery};

    #[test]
    fn similarity_filters_are_bounded_and_normalized() {
        let (problem_id, language, minimum) = SimilarityQuery {
            problem_id: Some(7),
            language: Some(" Cpp ".to_owned()),
            min_group_size: 3,
        }
        .validate()
        .expect("valid similarity filters");
        assert_eq!(problem_id, Some(7));
        assert_eq!(language.as_deref(), Some("cpp"));
        assert_eq!(minimum, 3);
        assert!(
            SimilarityQuery { problem_id: None, language: None, min_group_size: 1 }
                .validate()
                .is_err()
        );
    }

    #[test]
    fn similarity_pair_threshold_matches_displayed_percentage() {
        let (_, _, min_similarity_percent) = SimilarityPairQuery {
            problem_id: None,
            language: Some("CPP".to_owned()),
            min_similarity_percent: 85,
        }
        .validate()
        .expect("valid pair filters");
        assert_eq!(min_similarity_percent, 85);
        assert!(
            SimilarityPairQuery { problem_id: None, language: None, min_similarity_percent: 49 }
                .validate()
                .is_err()
        );
    }
}
