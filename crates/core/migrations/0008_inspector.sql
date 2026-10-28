-- A link someone removed by hand: the matcher doesn't make it again. Linking the
-- file to that item by hand removes the row.
CREATE TABLE link_rejection (
    file_id    INTEGER NOT NULL REFERENCES media_file (id) ON DELETE CASCADE,
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (file_id, item_id)
) STRICT;

-- An item's history: its own events and its files'.
CREATE INDEX event_item ON event (json_extract(payload, '$.item_id'));
CREATE INDEX event_file ON event (json_extract(payload, '$.file_id'));
