-- Guests: people trying a demo server without an account (docs/guests.md).

-- A throwaway account made by "Continue as guest". Removed with its last session.
ALTER TABLE app_user ADD COLUMN is_guest INTEGER NOT NULL DEFAULT 0 CHECK (is_guest IN (0, 1));

-- Whether the login page and apps offer "Continue as guest". Off by default.
ALTER TABLE auth_setting ADD COLUMN guest_access INTEGER NOT NULL DEFAULT 0 CHECK (guest_access IN (0, 1));
