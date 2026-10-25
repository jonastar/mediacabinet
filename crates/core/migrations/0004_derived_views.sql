-- Derived state as views: availability, resolved monitoring, quality and wanted.
-- Computed on every read from links, file status, streams and dates; never stored.

-- Series: limit wanted to the episodes of one playlist (e.g. no filler).
ALTER TABLE item ADD COLUMN wanted_playlist_id INTEGER REFERENCES playlist (id) ON DELETE SET NULL;

-- The wanted set as of the last `item.wanted` / `item.unwanted` events. Used only
-- to tell what changed since; `item_wanted` stays the source of truth.
CREATE TABLE wanted_snapshot (
    item_id INTEGER PRIMARY KEY REFERENCES item (id) ON DELETE CASCADE
) STRICT;

-- Profiles carry a resolution cutoff: {"cutoff": "1080p"}. Files below it are
-- wanted as upgrades. No cutoff means any file will do.
INSERT INTO quality_profile (name, rules)
SELECT name, rules FROM (
    SELECT 'Any' AS name, '{}' AS rules
    UNION ALL SELECT 'HD 720p', '{"cutoff": "720p"}'
    UNION ALL SELECT 'HD 1080p', '{"cutoff": "1080p"}'
    UNION ALL SELECT 'UHD 2160p', '{"cutoff": "2160p"}'
)
WHERE NOT EXISTS (SELECT 1 FROM quality_profile);

-- Per episode and movie: availability, resolved monitoring and quality.
--   availability: available | missing | not_acquired | upcoming | unknown
--   monitored: the item's own setting, else its series', else not monitored
--   best_height: the tallest video stream among present linked files
CREATE VIEW item_state AS
WITH files AS (
    SELECT l.item_id,
           sum(f.status = 'present') AS present,
           sum(f.status = 'missing') AS missing,
           max(CASE WHEN f.status = 'present' THEN (
               SELECT max(s.height) FROM media_stream s WHERE s.file_id = f.id AND s.kind = 'video'
           ) END) AS best_height
    FROM file_link l JOIN media_file f ON f.id = l.file_id
    GROUP BY l.item_id
)
SELECT i.id AS item_id,
       i.kind,
       i.parent_id,
       CASE
           WHEN coalesce(fi.present, 0) > 0 THEN 'available'
           WHEN coalesce(fi.missing, 0) > 0 THEN 'missing'
           WHEN i.airs_at IS NULL THEN 'unknown'
           WHEN i.airs_at > unixepoch() THEN 'upcoming'
           ELSE 'not_acquired'
       END AS availability,
       coalesce(m.monitored, pm.monitored, 0) AS monitored,
       coalesce(m.quality_profile_id, pm.quality_profile_id) AS quality_profile_id,
       fi.best_height
FROM item i
LEFT JOIN files fi ON fi.item_id = i.id
LEFT JOIN monitor m ON m.item_id = i.id
LEFT JOIN monitor pm ON pm.item_id = i.parent_id
WHERE i.kind IN ('episode', 'movie');

-- Items that are wanted, and why:
--   missing / not_acquired: monitored and not available
--   upgrade: available, but below the quality profile's cutoff
-- A series with a wanted playlist only wants that playlist's episodes.
CREATE VIEW item_wanted AS
SELECT st.item_id,
       st.kind,
       st.parent_id,
       st.availability,
       st.quality_profile_id,
       st.best_height,
       q.cutoff_height,
       CASE WHEN st.availability = 'available' THEN 'upgrade' ELSE st.availability END AS reason
FROM item_state st
LEFT JOIN (
    SELECT id,
           CASE json_extract(rules, '$.cutoff')
               WHEN '2160p' THEN 2160
               WHEN '1440p' THEN 1440
               WHEN '1080p' THEN 1080
               WHEN '720p' THEN 720
               WHEN '576p' THEN 576
               WHEN '480p' THEN 480
           END AS cutoff_height
    FROM quality_profile
) q ON q.id = st.quality_profile_id
LEFT JOIN item series ON series.id = st.parent_id
LEFT JOIN playlist wp ON wp.id = series.wanted_playlist_id
WHERE st.monitored
  AND (
      st.availability IN ('missing', 'not_acquired')
      -- Small allowance: 1080p releases are often 1040 or 800 lines tall (wide aspect ratios).
      OR (st.availability = 'available' AND q.cutoff_height IS NOT NULL
          AND coalesce(st.best_height, 0) < q.cutoff_height * 0.7)
  )
  AND (
      wp.id IS NULL
      OR (wp.kind = 'manual' AND EXISTS (
          SELECT 1 FROM playlist_entry pe WHERE pe.playlist_id = wp.id AND pe.item_id = st.item_id))
      OR (wp.kind = 'smart'
          AND EXISTS (SELECT 1 FROM ordering_entry oe
                      WHERE oe.ordering_id = wp.base_ordering_id AND oe.item_id = st.item_id)
          AND NOT EXISTS (SELECT 1 FROM item_tag t, json_each(wp.rules, '$.exclude_tags') x
                          WHERE t.item_id = st.item_id AND t.tag = x.value))
  );
