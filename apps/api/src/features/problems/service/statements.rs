use super::*;

impl ProblemService {
    pub async fn upsert_statement(
        &self,
        problem_id: i64,
        statement: ValidatedStatement,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemStatementResponse, AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem statement update", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let row = sqlx::query_as::<_, ProblemStatementRow>(
            r#"
                INSERT INTO problem_statements (problem_id, lang_code, body, updated_at)
                VALUES ($1, $2, $3, now())
                ON CONFLICT (problem_id, lang_code)
                DO UPDATE SET body = EXCLUDED.body, updated_at = now()
                RETURNING problem_id, lang_code, body, updated_at
                "#,
        )
        .bind(problem_id)
        .bind(statement.lang_code)
        .bind(statement.body)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("upsert problem statement", error))?;
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch statement problem", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_STATEMENT_UPSERTED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem statement update", error))?;
        Ok(ProblemStatementResponse {
            problem_id: row.problem_id,
            lang_code: row.lang_code,
            rendered_html: render_safe(&row.body),
            body: row.body,
            updated_at: row.updated_at,
        })
    }

    pub async fn list_statements(
        &self,
        problem_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ProblemStatementResponse>, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        let rows = sqlx::query_as::<_, ProblemStatementRow>(
            r#"
                SELECT problem_id, lang_code, body, updated_at
                FROM problem_statements
                WHERE problem_id = $1
                ORDER BY lang_code
                "#,
        )
        .bind(problem_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list problem statements", error))?;
        Ok(rows
            .into_iter()
            .map(|row| ProblemStatementResponse {
                problem_id: row.problem_id,
                lang_code: row.lang_code,
                rendered_html: render_safe(&row.body),
                body: row.body,
                updated_at: row.updated_at,
            })
            .collect())
    }

    pub async fn delete_statement(
        &self,
        problem_id: i64,
        lang_code: String,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem statement deletion", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let deleted =
            sqlx::query("DELETE FROM problem_statements WHERE problem_id = $1 AND lang_code = $2")
                .bind(problem_id)
                .bind(lang_code)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("delete problem statement", error))?
                .rows_affected();
        if deleted == 0 {
            return Err(AppError::not_found(
                "STATEMENT_NOT_FOUND",
                "Problem statement was not found",
            ));
        }
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch problem after statement deletion", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_STATEMENT_DELETED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem statement deletion", error))
    }
}
