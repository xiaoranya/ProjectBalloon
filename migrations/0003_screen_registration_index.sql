-- Screen registration is rate limited per client address over a rolling window.
--
-- `screen_instances.last_ip` had no index, so every registration request scanned
-- every instance ever created. The composite index also serves the window filter
-- (`created_at > now() - interval '10 minutes'`).

CREATE INDEX idx_screen_instances_last_ip_created
    ON screen_instances (last_ip, created_at);
