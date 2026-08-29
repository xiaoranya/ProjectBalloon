use std::collections::HashMap;
use std::net::IpAddr;

use time::OffsetDateTime;

use crate::{error::AppError, features::auth::model::AuthUser};

use crate::features::awards::model::{
    HostScriptRequest, HostScriptResponse, HostScriptSectionResponse, PresentationCategory,
    PresentationRecipient, PresentationRequest, PresentationResponse,
};
use crate::features::awards::service::{
    AwardService, audit, require_active_contest, require_operator,
};

impl AwardService {
    pub async fn presentation(&self, contest: i64) -> Result<PresentationResponse, AppError> {
        let (contest_name, contest_status) = sqlx::query_as::<_, (String, String)>(
            "SELECT name,status FROM contests WHERE id=$1 AND deleted_at IS NULL AND visibility='PUBLIC'",
        )
        .bind(contest)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation contest", error))?
        .ok_or_else(|| AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))?;
        let frozen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
        )
        .bind(contest)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check frozen award presentation", error))?;
        if !frozen {
            return Err(AppError::not_found(
                "AWARD_PRESENTATION_NOT_READY",
                "A frozen award set is required",
            ));
        }
        let mut categories = sqlx::query_as::<_, PresentationCategory>(
            "SELECT id,code,name,display_order,group_name,first_blood FROM award_categories WHERE contest_id=$1 ORDER BY display_order,id",
        )
        .bind(contest)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation categories", error))?;
        if categories.is_empty() {
            return Err(AppError::not_found(
                "AWARD_PRESENTATION_NOT_READY",
                "A frozen award set with categories is required",
            ));
        }
        // Load the category key alongside each public recipient, then group the
        // flattened rows into category sections.
        let recipients = sqlx::query_as::<_, (i64, i64, Option<i64>, Option<String>, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<i32>, Option<i32>, Option<i64>)>(
            "SELECT category_id,id,problem_id,problem_alias,team_id,coalesce(team_name,''),school,seat_no,group_name,participation_type,is_star,rank,solved,penalty_minutes FROM award_recipients WHERE contest_id=$1 ORDER BY rank NULLS LAST,team_id,id",
        )
        .bind(contest)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation recipients", error))?;
        let mut recipients_by_category = HashMap::<i64, Vec<PresentationRecipient>>::new();
        for (
            category_id,
            id,
            problem_id,
            problem_alias,
            team_id,
            team_name,
            school,
            seat_no,
            group_name,
            participation_type,
            star,
            rank,
            solved,
            penalty_minutes,
        ) in recipients
        {
            recipients_by_category.entry(category_id).or_default().push(PresentationRecipient {
                id,
                problem_id,
                problem_alias,
                team_id,
                team_name,
                school,
                seat_no,
                group_name,
                participation_type,
                star,
                rank,
                solved,
                penalty_minutes,
            });
        }
        for category in &mut categories {
            category.recipients = recipients_by_category.remove(&category.id).unwrap_or_default();
        }
        let state = sqlx::query_as::<_, (Option<i64>, String, bool, i32, OffsetDateTime)>(
            "SELECT current_category_id,status,auto_rotate,interval_seconds,updated_at FROM award_presentation_states WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation state", error))?;
        let now = OffsetDateTime::now_utc();
        let first_category = categories[0].id;
        let (current, status, auto_rotate, interval, updated_at) = state.map_or(
            (first_category, "WAITING".to_owned(), false, 15, now),
            |(current, status, auto_rotate, interval, updated_at)| {
                let current = current
                    .filter(|id| categories.iter().any(|category| category.id == *id))
                    .unwrap_or(first_category);
                (current, status, auto_rotate, interval, updated_at)
            },
        );
        Ok(PresentationResponse {
            contest_id: contest,
            contest_name,
            contest_status,
            server_time: now,
            status,
            current_category_id: current,
            auto_rotate,
            interval_seconds: interval,
            state_updated_at: updated_at,
            categories,
        })
    }

    pub async fn update_presentation(
        &self,
        contest: i64,
        mut request: PresentationRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<PresentationResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        request.status = request.status.trim().to_ascii_uppercase();
        if !matches!(request.status.as_str(), "WAITING" | "PRESENTING" | "COMPLETED") {
            return Err(AppError::validation("status", "is not a presentation status"));
        }
        if !(5..=120).contains(&request.interval_seconds) {
            return Err(AppError::validation("intervalSeconds", "must be between 5 and 120"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award presentation update", error))?;
        let category_ids = sqlx::query_scalar::<_, i64>(
            "SELECT c.id FROM award_categories c JOIN award_sets s ON s.contest_id=c.contest_id AND s.status='FROZEN' WHERE c.contest_id=$1 ORDER BY c.display_order,c.id",
        )
        .bind(contest)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load frozen presentation categories", error))?;
        let category_id = request
            .current_category_id
            .or_else(|| category_ids.first().copied())
            .ok_or_else(|| {
                AppError::conflict("AWARD_PRESENTATION_NOT_READY", "A frozen award set is required")
            })?;
        if !category_ids.contains(&category_id) {
            return Err(AppError::validation(
                "currentCategoryId",
                "AWARD_PRESENTATION_CATEGORY_NOT_FROZEN",
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO award_presentation_states
                (contest_id,current_category_id,status,auto_rotate,interval_seconds,
                 updated_by_user_id)
            VALUES($1,$2,$3,$4,$5,$6)
            ON CONFLICT(contest_id) DO UPDATE
                SET current_category_id=excluded.current_category_id,
                    status=excluded.status,
                    auto_rotate=excluded.auto_rotate,
                    interval_seconds=excluded.interval_seconds,
                    updated_by_user_id=excluded.updated_by_user_id,
                    updated_at=now(),
                    version=award_presentation_states.version+1
            "#,
        )
        .bind(contest)
        .bind(category_id)
        .bind(&request.status)
        .bind(request.auto_rotate)
        .bind(request.interval_seconds)
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("save award presentation state", error))?;
        audit(&mut tx, actor.id, "AWARD_PRESENTATION_UPDATED", contest, ip).await?;
        sqlx::query("INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,payload_json) VALUES($1,$2,'AWARDS_UPDATED','PUBLIC',$3)")
            .bind(uuid::Uuid::new_v4()).bind(contest).bind(serde_json::json!({"categoryId":category_id,"status":request.status}))
            .execute(&mut *tx).await.map_err(|error| AppError::internal("publish award presentation update", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award presentation update", error))?;
        self.presentation(contest).await
    }

    pub async fn host_script(&self, contest: i64) -> Result<HostScriptResponse, AppError> {
        let presentation = self.presentation(contest).await.map_err(map_host_script_not_ready)?;
        self.shape_host_script(presentation).await
    }

    pub async fn save_host_script(
        &self,
        contest: i64,
        request: HostScriptRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<HostScriptResponse, AppError> {
        require_operator(actor)?;
        if request.opening_text.chars().count() > 4000
            || request.closing_text.chars().count() > 4000
            || request.sections.len() > 100
            || request.sections.iter().any(|section| section.cue_text.chars().count() > 2000)
        {
            return Err(AppError::validation("hostScript", "contains text over the size limit"));
        }
        let presentation = self.presentation(contest).await.map_err(map_host_script_not_ready)?;
        let category_ids =
            presentation.categories.iter().map(|category| category.id).collect::<Vec<_>>();
        let mut cues = std::collections::HashMap::new();
        for section in request.sections {
            if !category_ids.contains(&section.category_id) {
                return Err(AppError::validation(
                    "categoryId",
                    "AWARD_HOST_SCRIPT_CATEGORY_INVALID",
                ));
            }
            if cues.insert(section.category_id, section.cue_text).is_some() {
                return Err(AppError::validation(
                    "categoryId",
                    "AWARD_HOST_SCRIPT_CATEGORY_DUPLICATE",
                ));
            }
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin host script update", error))?;
        let existing = sqlx::query_as::<_, (i64, i64)>(
            "SELECT id,version FROM award_host_scripts WHERE contest_id=$1 FOR UPDATE",
        )
        .bind(contest)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock host script", error))?;
        let script_id = match existing {
            None if request.expected_version.is_none() => sqlx::query_scalar::<_, i64>("INSERT INTO award_host_scripts(contest_id,opening_text,closing_text,updated_by_user_id) VALUES($1,$2,$3,$4) RETURNING id")
                .bind(contest).bind(&request.opening_text).bind(&request.closing_text).bind(actor.id).fetch_one(&mut *tx).await.map_err(|error| AppError::internal("create host script", error))?,
            Some((id, version)) if request.expected_version == Some(version) => {
                sqlx::query("UPDATE award_host_scripts SET opening_text=$2,closing_text=$3,updated_by_user_id=$4,updated_at=now(),version=version+1 WHERE id=$1")
                    .bind(id).bind(&request.opening_text).bind(&request.closing_text).bind(actor.id).execute(&mut *tx).await.map_err(|error| AppError::internal("update host script", error))?;
                id
            }
            _ => return Err(AppError::conflict("AWARD_HOST_SCRIPT_VERSION_CONFLICT", "Host script changed; reload and retry")),
        };
        sqlx::query("DELETE FROM award_host_script_sections WHERE host_script_id=$1")
            .bind(script_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("replace host script sections", error))?;
        for (index, category) in presentation.categories.iter().enumerate() {
            let cue = cues.remove(&category.id).unwrap_or_else(|| default_cue(category));
            sqlx::query("INSERT INTO award_host_script_sections(host_script_id,category_id,cue_text,display_order) VALUES($1,$2,$3,$4)")
                .bind(script_id).bind(category.id).bind(cue).bind(i32::try_from(index + 1).unwrap_or(i32::MAX)).execute(&mut *tx).await.map_err(|error| AppError::internal("save host script section", error))?;
        }
        audit(&mut tx, actor.id, "AWARD_HOST_SCRIPT_UPDATED", contest, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit host script update", error))?;
        self.shape_host_script(presentation).await
    }

    pub async fn shape_host_script(
        &self,
        presentation: PresentationResponse,
    ) -> Result<HostScriptResponse, AppError> {
        let script = sqlx::query_as::<_, (i64, String, String, i64, OffsetDateTime)>("SELECT id,opening_text,closing_text,version,updated_at FROM award_host_scripts WHERE contest_id=$1")
            .bind(presentation.contest_id).fetch_optional(&self.database).await.map_err(|error| AppError::internal("load host script", error))?;
        let cues = if let Some((id, _, _, _, _)) = &script {
            sqlx::query_as::<_, (i64, String)>("SELECT category_id,cue_text FROM award_host_script_sections WHERE host_script_id=$1 ORDER BY display_order")
                .bind(id).fetch_all(&self.database).await.map_err(|error| AppError::internal("load host script sections", error))?.into_iter().collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };
        let current_index = presentation
            .categories
            .iter()
            .position(|category| category.id == presentation.current_category_id)
            .unwrap_or(0);
        let next_category_id =
            presentation.categories.get(current_index + 1).map(|category| category.id);
        let sections = presentation
            .categories
            .iter()
            .map(|category| HostScriptSectionResponse {
                category_id: category.id,
                code: category.code.clone(),
                name: category.name.clone(),
                first_blood: category.first_blood,
                current: category.id == presentation.current_category_id,
                cue_text: cues.get(&category.id).cloned().unwrap_or_else(|| default_cue(category)),
                recipients: category.recipients.clone(),
            })
            .collect();
        let (version, updated_at, opening_text, closing_text) = script.map_or_else(
            || {
                (
                    None,
                    None,
                    format!("各位嘉宾、参赛选手，{}颁奖典礼现在开始。", presentation.contest_name),
                    "祝贺所有获奖队伍，感谢各位嘉宾与参赛选手。颁奖典礼到此结束。".to_owned(),
                )
            },
            |(_, opening, closing, version, updated)| {
                (Some(version), Some(updated), opening, closing)
            },
        );
        Ok(HostScriptResponse {
            contest_id: presentation.contest_id,
            contest_name: presentation.contest_name,
            server_time: presentation.server_time,
            presentation_status: presentation.status,
            current_category_id: presentation.current_category_id,
            next_category_id,
            auto_rotate: presentation.auto_rotate,
            interval_seconds: presentation.interval_seconds,
            state_updated_at: presentation.state_updated_at,
            version,
            updated_at,
            opening_text,
            closing_text,
            sections,
        })
    }
}

fn default_cue(category: &PresentationCategory) -> String {
    let verb = if category.first_blood { "公布" } else { "颁发" };
    format!("接下来{verb}{}，请获奖队伍代表上台领奖。", category.name)
}

fn map_host_script_not_ready(error: AppError) -> AppError {
    if error.code() == "AWARD_PRESENTATION_NOT_READY" {
        AppError::conflict("AWARD_HOST_SCRIPT_NOT_READY", "A frozen award set is required")
    } else {
        error
    }
}
