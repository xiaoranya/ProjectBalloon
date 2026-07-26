-- Access-control cleanup must remain possible after a contest is archived.
-- Archived business data is immutable, but removing an obsolete administrator
-- assignment is not a business-data mutation and is required for role/scope
-- maintenance.
DROP TRIGGER IF EXISTS trg_contest_admin_assignments_archived_read_only
    ON contest_admin_assignments;

CREATE TRIGGER trg_contest_admin_assignments_archived_read_only
BEFORE INSERT OR UPDATE ON contest_admin_assignments
FOR EACH ROW EXECUTE FUNCTION reject_archived_contest_write();
