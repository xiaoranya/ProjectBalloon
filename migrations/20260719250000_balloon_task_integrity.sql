UPDATE balloon_tasks SET status = upper(status);
UPDATE balloon_tasks task
SET color = coalesce(nullif(btrim(task.color), ''), nullif(btrim(problem.color), ''), 'UNSET')
FROM contest_problems problem
WHERE problem.contest_id = task.contest_id AND problem.problem_id = task.problem_id;
UPDATE balloon_tasks SET color = 'UNSET' WHERE color IS NULL OR btrim(color) = '';
UPDATE balloon_tasks SET status = 'PENDING' WHERE status NOT IN ('PENDING', 'CLAIMED', 'DELIVERED', 'CANCELLED');
UPDATE balloon_tasks SET status = 'PENDING'
WHERE status IN ('CLAIMED', 'DELIVERED') AND claimed_by IS NULL;
UPDATE balloon_tasks SET claimed_by = NULL, claimed_at = NULL, delivered_at = NULL,
    cancelled_at = NULL, cancelled_reason = NULL WHERE status = 'PENDING';
UPDATE balloon_tasks SET claimed_at = coalesce(claimed_at, updated_at), delivered_at = NULL,
    cancelled_at = NULL, cancelled_reason = NULL WHERE status = 'CLAIMED';
UPDATE balloon_tasks SET claimed_at = coalesce(claimed_at, updated_at),
    delivered_at = coalesce(delivered_at, updated_at), cancelled_at = NULL,
    cancelled_reason = NULL WHERE status = 'DELIVERED';
UPDATE balloon_tasks SET delivered_at = NULL, cancelled_at = coalesce(cancelled_at, updated_at),
    cancelled_reason = coalesce(nullif(btrim(cancelled_reason), ''), 'legacy cancellation'),
    claimed_at = CASE WHEN claimed_by IS NULL THEN NULL ELSE coalesce(claimed_at, updated_at) END
WHERE status = 'CANCELLED';

WITH ranked AS (
    SELECT task.id, row_number() OVER (
        PARTITION BY task.contest_id, task.problem_id
        ORDER BY submission.submitted_at, task.team_id, task.submission_id, task.id
    ) AS position
    FROM balloon_tasks task
    JOIN submissions submission ON submission.id = task.submission_id
    WHERE task.is_first_blood
)
UPDATE balloon_tasks task SET is_first_blood = false
FROM ranked WHERE task.id = ranked.id AND ranked.position > 1;

ALTER TABLE balloon_tasks
    ALTER COLUMN status SET DEFAULT 'PENDING',
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD COLUMN reopened_count integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT balloon_task_status_known
        CHECK (status IN ('PENDING', 'CLAIMED', 'DELIVERED', 'CANCELLED')),
    ADD CONSTRAINT balloon_task_color_present
        CHECK (color IS NOT NULL AND char_length(btrim(color)) BETWEEN 1 AND 16),
    ADD CONSTRAINT balloon_task_note_bounds
        CHECK (note IS NULL OR char_length(note) <= 2000),
    ADD CONSTRAINT balloon_task_reopened_count_valid CHECK (reopened_count >= 0),
    ADD CONSTRAINT balloon_task_state_shape CHECK (
        (status = 'PENDING' AND claimed_by IS NULL AND claimed_at IS NULL
            AND delivered_at IS NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'CLAIMED' AND claimed_by IS NOT NULL AND claimed_at IS NOT NULL
            AND delivered_at IS NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'DELIVERED' AND claimed_by IS NOT NULL AND claimed_at IS NOT NULL
            AND delivered_at IS NOT NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'CANCELLED' AND delivered_at IS NULL AND cancelled_at IS NOT NULL
            AND cancelled_reason IS NOT NULL
            AND ((claimed_by IS NULL AND claimed_at IS NULL)
                OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL)))
    );

CREATE UNIQUE INDEX idx_balloon_tasks_first_blood_unique
    ON balloon_tasks (contest_id, problem_id) WHERE is_first_blood;

CREATE INDEX idx_balloon_tasks_workbench
    ON balloon_tasks (contest_id, status, is_first_blood DESC, created_at, id);
