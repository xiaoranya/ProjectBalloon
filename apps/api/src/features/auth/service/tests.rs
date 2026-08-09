use super::crypto::{access_fingerprint, constant_time_equal};
use crate::features::auth::model::{AuthUser, UserType};

#[test]
fn access_fingerprint_is_deterministic() {
    let user = AuthUser {
        id: 1,
        username: "admin".to_owned(),
        display_name: "Admin".to_owned(),
        user_type: UserType::Staff,
        permissions: vec!["CLARIFICATION_MANAGE".to_owned()],
        password_reset_required: false,
    };

    assert_eq!(access_fingerprint(&user), access_fingerprint(&user));
    let mut changed = user.clone();
    changed.permissions = vec!["PRINTING_MANAGE".to_owned()];
    assert_ne!(access_fingerprint(&user), access_fingerprint(&changed));
    assert!(constant_time_equal("same", "same"));
    assert!(!constant_time_equal("same", "different"));
}
