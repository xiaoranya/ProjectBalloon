-- Rust-managed staff account creation requires every staff UserType to have a
-- deterministic role. The Java implementation created SUPER_ADMIN at runtime;
-- fresh Rust installations seed it through a migration instead.
INSERT INTO roles (code, name, description, builtin)
VALUES (
    'SUPER_ADMIN',
    'Super Administrator',
    'Full platform administration access',
    true
)
ON CONFLICT (code) DO NOTHING;
