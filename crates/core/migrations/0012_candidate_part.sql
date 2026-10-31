-- Which part of an episode a file candidate is, when the episode is split over files.
ALTER TABLE match_candidate ADD COLUMN part INTEGER;
