use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use utoipa::ToSchema;

const MAX_CONTESTS_PER_ADMIN: usize = 1_000;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceContestManagementScopeRequest {
    pub contest_ids: Vec<i64>,
}

impl ReplaceContestManagementScopeRequest {
    pub fn validate(self) -> Result<Vec<i64>, AppError> {
        if self.contest_ids.len() > MAX_CONTESTS_PER_ADMIN {
            return Err(AppError::validation("contestIds", "must contain at most 1000 entries"));
        }
        if self.contest_ids.iter().any(|contest_id| *contest_id <= 0) {
            return Err(AppError::validation("contestIds", "must contain only positive IDs"));
        }
        Ok(self.contest_ids.into_iter().collect::<BTreeSet<_>>().into_iter().collect())
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ContestManagementScopeResponse {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub enabled: bool,
    pub contest_ids: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use super::ReplaceContestManagementScopeRequest;

    #[test]
    fn scope_ids_are_sorted_and_deduplicated() {
        let ids = ReplaceContestManagementScopeRequest { contest_ids: vec![3, 1, 3, 2] }
            .validate()
            .expect("valid IDs");
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn non_positive_scope_ids_are_rejected() {
        assert!(ReplaceContestManagementScopeRequest { contest_ids: vec![0] }.validate().is_err());
    }
}
