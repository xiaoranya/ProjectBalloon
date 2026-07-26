-- Default audit browsing is reverse chronological. Staff and scope screens
-- filter by user type and then order by username.
CREATE INDEX idx_audit_logs_created_at
    ON audit_logs (created_at DESC, id DESC);

CREATE INDEX idx_users_user_type_username
    ON users (user_type, username, id);
