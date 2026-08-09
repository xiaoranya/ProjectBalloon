INSERT INTO permissions (code, name, description) VALUES
    ('CONTEST_MANAGE', 'Contest management', 'Manage assigned contests, teams, problems, submissions, announcements, scoring, and scoreboards'),
    ('CLARIFICATION_MANAGE', 'Clarification management', 'Review and answer contest clarifications'),
    ('PRINTING_MANAGE', 'Printing management', 'Operate the contest print queue'),
    ('BALLOON_MANAGE', 'Balloon management', 'Operate balloon dispatch and delivery'),
    ('RESOLVER_MANAGE', 'Resolver management', 'Operate official scoreboard resolver runs'),
    ('AWARD_MANAGE', 'Award management', 'Manage awards and award presentation'),
    ('SCREEN_MANAGE', 'Screen management', 'Configure venue presentation screens'),
    ('LIVE_MANAGE', 'Live management', 'Configure public broadcast presentation pages');

CREATE TABLE user_permissions (
    user_id bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id bigint NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, permission_id)
);
