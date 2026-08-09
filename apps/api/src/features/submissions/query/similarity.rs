use sha2::Digest;

use crate::{
    error::AppError, features::auth::model::AuthUser, object_storage::ObjectStorageHandle,
};

use super::super::service::SubmissionService;
use super::{
    SimilarityBackfillResponse, SimilarityGroupResponse, SimilarityPairQuery,
    SimilarityPairResponse, SimilarityQuery, require_admin_access,
};

impl SubmissionService {
    pub async fn backfill_similarity(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<SimilarityBackfillResponse, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let candidates = sqlx::query_as::<_, (i64, String, String)>(
            r#"
            SELECT id, source_object_key, source_sha256
            FROM submissions
            WHERE contest_id = $1 AND source_simhash IS NULL AND source_sha256 IS NOT NULL
            ORDER BY id
            LIMIT 1000
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load submissions for similarity backfill", error))?;
        let scanned = i64::try_from(candidates.len()).unwrap_or(i64::MAX);
        let mut updated = 0_i64;
        let mut failed = 0_i64;
        for (submission_id, object_key, expected_hash) in candidates {
            let source = match storage
                .backend()
                .get_limited(
                    storage.source_bucket(),
                    &object_key,
                    super::super::model::MAX_SOURCE_BYTES,
                )
                .await
            {
                Ok(source) if hex::encode(sha2::Sha256::digest(&source)) == expected_hash => source,
                _ => {
                    failed += 1;
                    continue;
                }
            };
            let signature = super::super::model::source_similarity_signature(&source);
            let fingerprint = super::super::model::source_fingerprint(&source);
            let changed = sqlx::query(
                "UPDATE submissions SET source_fingerprint = $2, source_simhash = $3, source_token_count = $4 WHERE id = $1 AND source_simhash IS NULL",
            )
            .bind(submission_id)
            .bind(fingerprint)
            .bind(signature.simhash)
            .bind(signature.token_count)
            .execute(&self.database)
            .await
            .map_err(|error| AppError::internal("persist similarity backfill", error))?
            .rows_affected();
            updated += i64::try_from(changed).unwrap_or(i64::MAX);
        }
        Ok(SimilarityBackfillResponse { scanned, updated, failed })
    }

    pub async fn list_similarity(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: SimilarityQuery,
    ) -> Result<Vec<SimilarityGroupResponse>, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let (problem_id, language, min_group_size) = query.validate()?;
        sqlx::query_as::<_, SimilarityGroupResponse>(
            r#"
            SELECT problem_id, language, source_fingerprint AS fingerprint,
                   array_agg(id ORDER BY submitted_at, id) AS submission_ids,
                   array_agg(team_id ORDER BY submitted_at, id) AS team_ids,
                   count(*) AS submission_count
            FROM submissions
            WHERE contest_id = $1 AND source_fingerprint IS NOT NULL
              AND ($2::bigint IS NULL OR problem_id = $2)
              AND ($3::text IS NULL OR language = $3)
            GROUP BY problem_id, language, source_fingerprint
            HAVING count(*) >= $4
            ORDER BY count(*) DESC, min(submitted_at), problem_id, language, source_fingerprint
            LIMIT 500
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(language)
        .bind(min_group_size)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list submission similarity groups", error))
    }

    pub async fn list_similarity_pairs(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: SimilarityPairQuery,
    ) -> Result<Vec<SimilarityPairResponse>, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let (problem_id, language, min_similarity_percent) = query.validate()?;
        sqlx::query_as::<_, SimilarityPairResponse>(
            r#"
            WITH pairs AS (
                SELECT a.problem_id, a.language,
                       a.id AS submission_id, a.team_id,
                       b.id AS other_submission_id, b.team_id AS other_team_id,
                       bit_count((a.source_simhash # b.source_simhash)::bit(64))::int AS hamming_distance
                FROM submissions a
                JOIN submissions b
                  ON b.contest_id = a.contest_id
                 AND b.problem_id = a.problem_id
                 AND b.language = a.language
                 AND b.id > a.id
                 AND b.team_id <> a.team_id
                WHERE a.contest_id = $1
                  AND a.source_simhash IS NOT NULL AND b.source_simhash IS NOT NULL
                  AND ($2::bigint IS NULL OR a.problem_id = $2)
                  AND ($3::text IS NULL OR a.language = $3)
            )
            SELECT problem_id, language, submission_id, team_id,
                   other_submission_id, other_team_id, hamming_distance,
                   round((100.0 * (64 - hamming_distance) / 64.0))::int AS similarity_percent
            FROM pairs
            WHERE round((100.0 * (64 - hamming_distance) / 64.0)) >= $4
            ORDER BY hamming_distance, problem_id, language, submission_id, other_submission_id
            LIMIT 1000
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(language)
        .bind(min_similarity_percent)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list submission similarity pairs", error))
    }
}
