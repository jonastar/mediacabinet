-- Every series and movie belongs to one library. The same show in two libraries
-- is two items, each with its own folders, files, monitoring and watch history.

-- Series and movies only; episodes follow their series.
ALTER TABLE item ADD COLUMN library_id INTEGER REFERENCES library (id) ON DELETE RESTRICT;

-- Series: the library of the primary folder, else any folder, else the first TV library.
UPDATE item SET library_id = coalesce(
    (SELECT r.library_id FROM series_location sl JOIN library_root r ON r.id = sl.root_id
     WHERE sl.series_item_id = item.id ORDER BY sl.is_primary DESC LIMIT 1),
    (SELECT id FROM library WHERE kind = 'tv' ORDER BY id LIMIT 1))
WHERE kind = 'series';

-- Movies: the library of a linked file, else the first movie library.
UPDATE item SET library_id = coalesce(
    (SELECT r.library_id FROM file_link l JOIN media_file f ON f.id = l.file_id
     JOIN library_root r ON r.id = f.root_id
     WHERE l.item_id = item.id ORDER BY f.status = 'present' DESC, f.status = 'removed' LIMIT 1),
    (SELECT id FROM library WHERE kind = 'movies' ORDER BY id LIMIT 1))
WHERE kind = 'movie';

CREATE INDEX item_library ON item (library_id) WHERE library_id IS NOT NULL;

-- Primary provider IDs were unique across all libraries; now within one
-- library and kind (a series and an episode may share a number).
DROP INDEX external_id_primary_unique;

CREATE TRIGGER external_id_primary_unique_in_library
BEFORE INSERT ON external_id
WHEN NEW.is_primary AND EXISTS (
    SELECT 1 FROM external_id x
    JOIN item o ON o.id = x.item_id
    LEFT JOIN item op ON op.id = o.parent_id
    JOIN item n ON n.id = NEW.item_id
    LEFT JOIN item np ON np.id = n.parent_id
    WHERE x.provider = NEW.provider AND x.value = NEW.value AND x.is_primary
      AND x.item_id <> NEW.item_id AND o.kind = n.kind
      AND coalesce(o.library_id, op.library_id) IS coalesce(n.library_id, np.library_id))
BEGIN
    SELECT RAISE(ABORT, 'primary external ID already used in this library');
END;

-- A series' folders are in roots of its own library.
CREATE TRIGGER series_location_in_library_insert
BEFORE INSERT ON series_location
WHEN (SELECT library_id FROM library_root WHERE id = NEW.root_id)
    IS NOT (SELECT library_id FROM item WHERE id = NEW.series_item_id)
BEGIN
    SELECT RAISE(ABORT, 'series location must be in the series'' library');
END;

CREATE TRIGGER series_location_in_library_update
BEFORE UPDATE OF root_id, series_item_id ON series_location
WHEN (SELECT library_id FROM library_root WHERE id = NEW.root_id)
    IS NOT (SELECT library_id FROM item WHERE id = NEW.series_item_id)
BEGIN
    SELECT RAISE(ABORT, 'series location must be in the series'' library');
END;

-- A file links only to items of the library its root is in.
CREATE TRIGGER file_link_in_library
BEFORE INSERT ON file_link
WHEN (SELECT r.library_id FROM media_file f JOIN library_root r ON r.id = f.root_id
      WHERE f.id = NEW.file_id)
    IS NOT (SELECT coalesce(i.library_id, p.library_id) FROM item i
            LEFT JOIN item p ON p.id = i.parent_id WHERE i.id = NEW.item_id)
BEGIN
    SELECT RAISE(ABORT, 'a file links only to items in its own library');
END;
