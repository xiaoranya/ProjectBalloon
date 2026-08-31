use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};

use crate::{error::AppError, features::auth::model::AuthUser};

use crate::features::awards::model::{
    CategoryRequest, CategoryResponse, RuleRequest, UpdateCategoryRequest,
};

mod presentation;
mod recipients;

#[cfg(test)]
pub(crate) use recipients::{certificate_value, select_rows};

pub struct AwardService {
    pub(super) database: PgPool,
}

impl AwardService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list_categories(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<CategoryResponse>, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        category_query(&self.database, contest).await
    }

    pub async fn create_category(
        &self,
        contest: i64,
        request: CategoryRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CategoryResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let request = validate_category(request)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin award category", e))?;
        ensure_awards_mutable(&mut tx, contest).await?;
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO award_categories (contest_id, code, name, display_order, include_star, group_name, participation_type, first_blood) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
            .bind(contest).bind(&request.code).bind(&request.name).bind(request.display_order)
            .bind(request.include_star).bind(request.group_name.as_deref())
            .bind(request.participation_type.as_deref()).bind(request.first_blood)
            .fetch_one(&mut *tx).await.map_err(map_category_error)?;
        insert_rule(&mut tx, id, &request.rule).await?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_CREATED", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award category", e))?;
        load_category(&self.database, id).await
    }

    pub async fn update_category(
        &self,
        id: i64,
        request: UpdateCategoryRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CategoryResponse, AppError> {
        require_operator(actor)?;
        let category = validate_category(request.category)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award category update", error))?;
        let contest = sqlx::query_scalar::<_, i64>(
            "SELECT contest_id FROM award_categories WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award category", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })?;
        require_active_contest_tx(&mut tx, contest).await?;
        ensure_awards_mutable(&mut tx, contest).await?;
        let changed = sqlx::query("UPDATE award_categories SET code=$2,name=$3,display_order=$4,include_star=$5,group_name=$6,participation_type=$7,first_blood=$8,updated_at=now(),version=version+1 WHERE id=$1 AND version=$9")
            .bind(id).bind(&category.code).bind(&category.name).bind(category.display_order)
            .bind(category.include_star).bind(category.group_name.as_deref())
            .bind(category.participation_type.as_deref()).bind(category.first_blood)
            .bind(request.expected_version).execute(&mut *tx).await.map_err(map_category_error)?
            .rows_affected();
        if changed != 1 {
            return Err(AppError::conflict(
                "AWARD_CATEGORY_VERSION_STALE",
                "Award category changed; reload and retry",
            ));
        }
        sqlx::query("DELETE FROM award_rules WHERE category_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("replace award category rule", error))?;
        insert_rule(&mut tx, id, &category.rule).await?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_UPDATED", id, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award category update", error))?;
        load_category(&self.database, id).await
    }

    pub async fn delete_category(
        &self,
        id: i64,
        expected_version: i32,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award category deletion", error))?;
        let (contest, version) = sqlx::query_as::<_, (i64, i32)>(
            "SELECT contest_id,version FROM award_categories WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award category", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })?;
        require_active_contest_tx(&mut tx, contest).await?;
        ensure_awards_mutable(&mut tx, contest).await?;
        if version != expected_version {
            return Err(AppError::conflict(
                "AWARD_CATEGORY_VERSION_STALE",
                "Award category changed; reload and retry",
            ));
        }
        sqlx::query("DELETE FROM award_recipients WHERE category_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete category recipients", error))?;
        sqlx::query("DELETE FROM award_categories WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete award category", error))?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_DELETED", id, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award category deletion", error))?;
        Ok(())
    }
}

pub(super) fn validate_category(mut r: CategoryRequest) -> Result<CategoryRequest, AppError> {
    r.code = r.code.trim().to_ascii_uppercase();
    r.name = r.name.trim().to_owned();
    r.group_name = r.group_name.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    r.participation_type = r.participation_type.map(|v| v.trim().to_ascii_uppercase());
    if r.code.is_empty()
        || r.code.len() > 64
        || !r.code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || r.name.is_empty()
        || r.name.chars().count() > 128
        || !(1..=1000).contains(&r.display_order)
    {
        return Err(AppError::validation(
            "category",
            "contains invalid code, name, or displayOrder",
        ));
    }
    if r.participation_type
        .as_ref()
        .is_some_and(|v| !matches!(v.as_str(), "OFFICIAL" | "STAR" | "PRACTICE"))
    {
        return Err(AppError::validation(
            "participationType",
            "must be OFFICIAL, STAR, or PRACTICE",
        ));
    }
    r.rule.rule_type = r.rule.rule_type.to_ascii_uppercase();
    let valid = match r.rule.rule_type.as_str() {
        "FIXED_COUNT" => {
            r.rule.fixed_count.is_some_and(|v| v > 0)
                && r.rule.ratio.is_none()
                && r.rule.rank_from.is_none()
                && r.rule.rank_to.is_none()
        }
        "RATIO" => {
            r.rule.ratio.is_some_and(|v| v > 0.0 && v <= 1.0)
                && r.rule.fixed_count.is_none()
                && r.rule.rank_from.is_none()
                && r.rule.rank_to.is_none()
        }
        "RANK_RANGE" => {
            r.rule.rank_from.zip(r.rule.rank_to).is_some_and(|(a, b)| a > 0 && b >= a)
                && r.rule.fixed_count.is_none()
                && r.rule.ratio.is_none()
        }
        _ => false,
    };
    if !valid {
        return Err(AppError::validation("rule", "contains an invalid award rule"));
    }
    Ok(r)
}

async fn insert_rule(
    tx: &mut Transaction<'_, Postgres>,
    category: i64,
    r: &RuleRequest,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO award_rules(category_id,rule_type,ratio,fixed_count,rank_from,rank_to) VALUES($1,$2,$3,$4,$5,$6)").bind(category).bind(&r.rule_type).bind(r.ratio).bind(r.fixed_count).bind(r.rank_from).bind(r.rank_to).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("insert award rule",e))
}

const CATEGORY_SQL: &str = r#"
    SELECT c.id,c.contest_id,c.code,c.name,c.display_order,c.include_star,c.group_name,
           c.participation_type,c.first_blood,c.version,r.rule_type,
           r.ratio::float8 AS ratio,r.fixed_count,r.rank_from,r.rank_to
    FROM award_categories c
    JOIN award_rules r
        ON r.category_id=c.id
"#;
async fn category_query(db: &PgPool, c: i64) -> Result<Vec<CategoryResponse>, AppError> {
    sqlx::query_as(safe_sql!("{CATEGORY_SQL} WHERE c.contest_id=$1 ORDER BY c.display_order,c.id"))
        .bind(c)
        .fetch_all(db)
        .await
        .map_err(|e| AppError::internal("list award categories", e))
}
async fn category_query_tx(
    tx: &mut Transaction<'_, Postgres>,
    c: i64,
) -> Result<Vec<CategoryResponse>, AppError> {
    sqlx::query_as(safe_sql!("{CATEGORY_SQL} WHERE c.contest_id=$1 ORDER BY c.display_order,c.id"))
        .bind(c)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AppError::internal("list award categories", e))
}
async fn load_category(db: &PgPool, id: i64) -> Result<CategoryResponse, AppError> {
    sqlx::query_as(safe_sql!("{CATEGORY_SQL} WHERE c.id=$1"))
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::internal("load award category", e))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })
}

async fn ensure_awards_mutable(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(), AppError> {
    require_active_contest_tx(tx, contest).await?;
    let frozen = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
    )
    .bind(contest)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::internal("check award freeze", e))?;
    if frozen {
        Err(AppError::conflict("AWARD_SET_FROZEN", "Frozen awards cannot be changed"))
    } else {
        Ok(())
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result)VALUES($1,$2,'AWARD',$3,$4,'success')").bind(actor).bind(action).bind(id.to_string()).bind(ip.to_string()).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("record award audit",e))
}

pub(super) fn require_operator(a: &AuthUser) -> Result<(), AppError> {
    if a.is_super_admin() || a.has_permission(crate::features::auth::permissions::AWARD_MANAGE) {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "AWARD_PERMISSION_REQUIRED",
            "Award management permission is required",
        ))
    }
}

async fn require_active_contest(database: &PgPool, contest: i64) -> Result<(), AppError> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
    )
    .bind(contest)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check award contest", error))?;
    if active {
        Ok(())
    } else {
        Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))
    }
}

async fn require_active_contest_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(), AppError> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
    )
    .bind(contest)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| AppError::internal("check award contest", error))?;
    if active {
        Ok(())
    } else {
        Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))
    }
}

fn stale() -> AppError {
    AppError::conflict("AWARD_VERSION_STALE", "Award set changed; reload and retry")
}

fn map_category_error(e: sqlx::Error) -> AppError {
    if e.as_database_error().and_then(sqlx::error::DatabaseError::constraint).is_some() {
        AppError::conflict("AWARD_CATEGORY_CONFLICT", "Award code or display order is already used")
    } else {
        AppError::internal("create award category", e)
    }
}

#[cfg(test)]
mod tests {
    use super::require_operator;
    use crate::features::auth::model::{UserType, user_for_test};
    use crate::features::auth::permissions;

    #[test]
    fn award_gate_accepts_super_admins_and_award_managers() {
        assert!(require_operator(&user_for_test(UserType::SuperAdmin, &[])).is_ok());
        assert!(
            require_operator(&user_for_test(UserType::Staff, &[permissions::AWARD_MANAGE])).is_ok()
        );
    }

    #[test]
    fn award_gate_rejects_operators_without_the_permission() {
        let error = require_operator(&user_for_test(UserType::Staff, &[]))
            .expect_err("missing permission must be rejected");
        assert_eq!(error.code(), "AWARD_PERMISSION_REQUIRED");
    }
}
