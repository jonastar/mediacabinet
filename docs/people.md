# People, companies and ratings

Who is in a series, episode or movie, who made it, where it airs and how it's rated. Status: built (`metadata/tmdb.rs`, `metadata/omdb.rs`, `catalog/people.rs`, migration 0014).

## Data

- **`person`**: someone a provider knows, one row per `(provider, provider_id)`, shared by every library. Name, photo path and department come with credits; biography, birth and death dates and birthplace are fetched from the provider the first time their page opens, and again after 90 days.
- **`credit`**: a person on an item, in billing order. `cast` is a series' regulars (with their episode count) or a movie's cast; `guest` is an episode's guest stars; `crew` is key crew only (directors, writers, creators, story, music, cinematography, editing, producers). Someone with two jobs has two rows.
- **`item_company`**: a series' networks, and studios of series and movies.
- **`item_rating`**: one score per source (`tmdb`, `imdb`, `rotten_tomatoes`, `metacritic`), stored with its scale (10 or 100) so the UI shows it as the site does.
- **`item.tagline`, `item.content_rating`**: plain fields, refreshed like the others.

Credits, companies and scores are replaced on each refresh and aren't edited by hand. Unchanged credits aren't rewritten, so daily refreshes of long series stay cheap. People nobody is credited as any more are deleted.

## Sources

- **TMDB**, in the requests import already makes: a series' cast from `aggregate_credits` (all seasons, at most 40), creators from `created_by`, each episode's guest stars and crew from its season, a movie's `credits`. Age ratings from `content_ratings` and `release_dates` in `CABINET_TMDB_REGION` (default US), else the US one. TMDB's own score is stored once something has votes.
- **OMDb** (optional, `CABINET_OMDB_KEY`) adds IMDb, the Rotten Tomatoes Tomatometer and Metacritic, by IMDb ID, for series and movies. Free keys allow 1,000 requests a day, so a background worker refreshes ratings weekly, oldest first, and pauses at the daily limit until the next round (every 10 minutes). A manual refresh also refreshes ratings. OMDb often has no Rotten Tomatoes or Metacritic score for series; the UI only shows the sources it has.

## API

- `GET /items/{id}/about`: tagline, age rating, ratings, networks, studios, cast, guests and crew for any series, episode or movie.
- `GET /people/{id}`: a person and everything in the library they're in, episodes folded into their series.
- `GET /people/{id}/photo`: their photo, cached like other artwork.

## Not yet

- Searching people by name, and people in the library search.
- An audience score for Rotten Tomatoes (OMDb doesn't have one; MDBList does, with its own key).
- Per-episode IMDb ratings (one OMDb request per episode is too many for free keys).
