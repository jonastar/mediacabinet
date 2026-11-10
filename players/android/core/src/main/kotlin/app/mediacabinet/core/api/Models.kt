// Generated from web/src/api/openapi.json by scripts/generate-api-clients.mjs. Do not edit.

package app.mediacabinet.core.api

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

/** What a series, episode or movie is beyond its title: people, companies and ratings. */
@Serializable
data class About(
    /** A series' regulars or a movie's cast, in billing order. */
    val cast: List<Credit> = emptyList(),
    /** The age rating, e.g. PG-13 or TV-MA. */
    @SerialName("content_rating") val contentRating: String? = null,
    /** Directors, writers, creators and other key crew; someone with two jobs is listed twice. */
    val crew: List<Credit> = emptyList(),
    /** An episode's guest stars. */
    val guests: List<Credit> = emptyList(),
    val networks: List<String> = emptyList(),
    /** IMDb, Rotten Tomatoes, Metacritic, then TMDB; only those known. */
    val ratings: List<Rating> = emptyList(),
    val studios: List<String> = emptyList(),
    val tagline: String? = null,
)

@Serializable
data class Accept(
    /** Pin the resulting link so rescans never change it. */
    val pin: Boolean? = null,
)

@Serializable
data class ActivityEntry(
    val actor: UserRef? = null,
    val at: Long,
    val audience: Audience,
    val id: Long,
    val item: ItemRef? = null,
    val kind: String,
    @SerialName("library_id") val libraryId: Long? = null,
    /**
     * Per kind. `items_added`: `items` (episode or movie IDs) and `new_series`.
     * Watching: `position_start`, `position_end`, `duration_seconds`.
     * `config_changed`: `target`, `action`, `id`, `name`, `fields`.
     * Tasks: `task_id`, `task_kind`, `trigger`, `error`, scope IDs.
     */
    val payload: JsonObject,
    val subject: UserRef? = null,
)

@Serializable
data class ActivityPage(
    val entries: List<ActivityEntry> = emptyList(),
    /** Pass as `before` for older entries; none when there are no more. */
    val next: String? = null,
)

@Serializable
data class AddItem(
    /**
     * The library to add it to. Defaults to the library of that kind with an
     * import target, else the first one.
     */
    @SerialName("library_id") val libraryId: Long? = null,
    val monitored: Boolean? = null,
    /** The metadata provider's ID (TMDB). */
    @SerialName("provider_id") val providerId: String,
    @SerialName("quality_profile_id") val qualityProfileId: Long? = null,
)

@Serializable
data class AddRoot(
    @SerialName("is_import_target") val isImportTarget: Boolean? = null,
    @SerialName("layout_template") val layoutTemplate: String,
    /** Absolute path on the server. */
    val path: String,
    /** `managed`, `adaptive` (default) or `read_only`. */
    @SerialName("write_policy") val writePolicy: String? = null,
)

@Serializable
data class ApiKey(
    @SerialName("created_at") val createdAt: Long,
    val id: Long,
    @SerialName("last_used_at") val lastUsedAt: Long? = null,
    val name: String,
    /** The key's first characters, to recognise it. */
    val prefix: String,
)

@Serializable
data class Attention(
    @SerialName("gap_series") val gapSeries: Long,
    val gaps: Long,
    /** Wanted items an external tool is working on. */
    @SerialName("in_progress") val inProgress: Long,
    @SerialName("missing_files") val missingFiles: Long,
    @SerialName("offline_roots") val offlineRoots: List<String> = emptyList(),
    val review: Long,
    @SerialName("status_sources") val statusSources: List<String> = emptyList(),
    val wanted: Long,
)

/** Who may see an entry. */
@Serializable
@JvmInline
value class Audience(val value: String) {
    companion object {
        val Everyone = Audience("everyone")
        val Admins = Audience("admins")
        val Subject = Audience("subject")
    }
}

@Serializable
data class AudioTrack(
    val channels: Long? = null,
    val codec: String? = null,
    val index: Long,
    @SerialName("is_default") val isDefault: Boolean = false,
    val language: String? = null,
)

@Serializable
data class AuthSettingsInput(
    /** Let people continue as a guest. Turning it off removes every guest. */
    @SerialName("guest_access") val guestAccess: Boolean? = null,
    /** Show the password form on the login page up front. */
    @SerialName("password_sign_in") val passwordSignIn: Boolean? = null,
)

@Serializable
data class AuthStatus(
    /** Whether people may continue as a guest (`POST /auth/guest`). */
    @SerialName("guest_access") val guestAccess: Boolean = false,
    /** Whether the login page shows the password form up front. */
    @SerialName("password_sign_in") val passwordSignIn: Boolean = false,
    /** Other accounts people can sign in with. */
    val providers: List<ProviderButton> = emptyList(),
    /** No account has a password yet: show the setup screen. */
    @SerialName("setup_needed") val setupNeeded: Boolean = false,
    /** The signed-in user, if any. A pending one may only sign out. */
    val user: User? = null,
)

@Serializable
@JvmInline
value class Availability(val value: String) {
    companion object {
        val Available = Availability("available")
        val Missing = Availability("missing")
        val NotAcquired = Availability("not_acquired")
        val Upcoming = Availability("upcoming")
        val Unknown = Availability("unknown")
    }
}

@Serializable
data class CalendarEntry(
    @SerialName("airs_at") val airsAt: Long,
    @SerialName("airs_time_known") val airsTimeKnown: Boolean = false,
    val availability: Availability,
    @SerialName("item_id") val itemId: Long,
    /** `episode` or `movie`. */
    val kind: String,
    val monitored: Boolean = false,
    val number: EpisodeNumber? = null,
    @SerialName("series_id") val seriesId: Long? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    val title: String,
    val wanted: Boolean = false,
)

@Serializable
data class Candidate(
    val confidence: Double,
    val id: Long,
    /** File candidates: the item. */
    @SerialName("item_id") val itemId: Long? = null,
    /** Folder candidates: the provider and its ID. */
    val provider: String? = null,
    @SerialName("provider_value") val providerValue: String? = null,
    val reason: String,
    /** `path_id`, `nfo`, `parser`, `search` or `ai`. */
    val source: String,
    /** A title to show: the provider's, or the episode's with its number. */
    val title: String,
)

/** What the provider says about the settings, without anyone signing in. */
@Serializable
data class CheckReport(
    /** The issuer as the provider names itself, if it was found. */
    val issuer: String? = null,
    val notes: List<Note> = emptyList(),
    /** The scopes a sign-in would ask for. */
    val scopes: String? = null,
)

@Serializable
data class CheckTemplate(
    /** `tv`, `movies` or `folders`. */
    val kind: String,
    val template: String,
)

/** The parts of a request that shape the decision. */
@Serializable
data class Choice(
    /** An audio stream index; defaults to the file's default audio. */
    @SerialName("audio_stream") val audioStream: Long? = null,
    /** Which file; defaults to the best present one. */
    @SerialName("file_id") val fileId: Long? = null,
    /** Transcode down to this height (a quality choice), e.g. 720. */
    @SerialName("max_height") val maxHeight: Long? = null,
    /** `stream:<index>` or `sidecar:<id>`; none for no subtitles. */
    val subtitle: String? = null,
)

@Serializable
data class Cleared(
    val removed: Long,
)

/**
 * What the client says it can play. Codec names are ffprobe's (`h264`, `hevc`,
 * `av1`, `vp9`, `aac`, `mp3`, `opus`, `flac`, `ac3`, `eac3`); containers are
 * `mp4`, `webm`, `mkv`.
 */
@Serializable
data class ClientCaps(
    @SerialName("audio_codecs") val audioCodecs: List<String>? = null,
    val containers: List<String>? = null,
    /**
     * The screen shows HDR (PQ/HLG). Otherwise HDR video is tone-mapped.
     * Ignored when `hdr_formats` lists any.
     */
    val hdr: Boolean? = null,
    /**
     * HDR formats the screen shows: `hdr10`, `hlg`, `dolby_vision`. Other HDR
     * video is tone-mapped.
     */
    @SerialName("hdr_formats") val hdrFormats: List<String>? = null,
    /** Audio with more channels than this is converted to stereo. No limit if unset. */
    @SerialName("max_audio_channels") val maxAudioChannels: Long? = null,
    @SerialName("video_codecs") val videoCodecs: List<String>? = null,
)

@Serializable
data class ComingUp(
    val episode: EpisodeRef,
    @SerialName("series_id") val seriesId: Long,
    @SerialName("series_title") val seriesTitle: String,
)

@Serializable
data class ConfirmBody(
    /** The matched account's password. */
    val password: String,
)

/** "Is this you?": an account whose verified email matched a user's. */
@Serializable
data class Confirmation(
    /** The Media Cabinet account with that email. */
    val account: String,
    /** Whether they can prove it with the account's password. */
    @SerialName("has_password") val hasPassword: Boolean = false,
    /** Or by signing in with a provider the account is already linked to. */
    @SerialName("other_providers") val otherProviders: List<ProviderButton> = emptyList(),
    /** The provider they signed in with. */
    val provider: String,
)

@Serializable
data class ContinueItem(
    val episode: EpisodeRef,
    @SerialName("last_played_at") val lastPlayedAt: Long,
    @SerialName("position_seconds") val positionSeconds: Double,
    @SerialName("series_id") val seriesId: Long,
    @SerialName("series_title") val seriesTitle: String,
    /** The ordering or playlist up next follows, when it isn't the series' default. */
    @SerialName("view_name") val viewName: String? = null,
)

@Serializable
data class CreateKey(
    /** What it's for, e.g. "Sonarr". */
    val name: String,
)

@Serializable
data class CreateLibrary(
    /** `tv`, `movies` or `folders`. */
    val kind: String,
    val name: String,
)

@Serializable
data class Credentials(
    val name: String,
    val password: String,
)

@Serializable
data class Credit(
    /** Crew only: Directing, Writing... */
    val department: String? = null,
    /** A series' cast: how many episodes they're in. */
    val episodes: Long? = null,
    /** The photo is at `/api/people/{person_id}/photo`. */
    @SerialName("has_photo") val hasPhoto: Boolean = false,
    val name: String,
    @SerialName("person_id") val personId: Long,
    /** The character for cast and guests, the job for crew. */
    val role: String? = null,
)

/** A TV or app signed in to an account. */
@Serializable
data class Device(
    val id: Long,
    @SerialName("last_seen_at") val lastSeenAt: Long,
    val name: String,
    @SerialName("signed_in_at") val signedInAt: Long,
)

@Serializable
data class DeviceCredentials(
    /** Shown in Account, where it can be signed out. */
    @SerialName("device_name") val deviceName: String,
    val name: String,
    val password: String,
)

@Serializable
data class DeviceSession(
    /** Send as `Authorization: Bearer`. Lasts a year from last use. */
    val token: String,
    val user: User,
)

@Serializable
data class DryRun(
    @SerialName("dry_run") val dryRun: Boolean? = null,
)

@Serializable
data class EditLink(
    /**
     * Which edition of the item the file is, such as "Director's Cut"; null or
     * empty for the only or usual one.
     */
    val edition: String? = null,
)

@Serializable
data class EnvInfo(
    /** The fields set by a variable; the rest have their defaults. */
    val fields: List<String> = emptyList(),
    /** The variables are no longer set. */
    val missing: Boolean = false,
    /** Edited here instead; the variables are ignored. */
    val overridden: Boolean = false,
    /**
     * e.g. `CABINET_OIDC_AUTHENTIK_`; a field's variable is this plus its
     * name in upper case.
     */
    val prefix: String,
)

@Serializable
data class Episode(
    /** Unix seconds, UTC. */
    @SerialName("airs_at") val airsAt: Long? = null,
    /** False when the provider only gave a date: show the date without a time. */
    @SerialName("airs_time_known") val airsTimeKnown: Boolean = false,
    /**
     * Artwork kinds available from `/api/artwork/{item_id}/{kind}`: `still`, the
     * episode's picture. The server chooses it: the provider's if there is one,
     * otherwise a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    @SerialName("item_id") val itemId: Long,
    val number: EpisodeNumber? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
    val availability: Availability,
    val files: List<LinkedFile> = emptyList(),
    @SerialName("is_filler") val isFiller: Boolean = false,
    @SerialName("is_gap") val isGap: Boolean = false,
    val monitored: Boolean = false,
    val overview: String? = null,
    val statuses: List<ExternalStatus> = emptyList(),
    val wanted: Boolean = false,
    /** Why it's wanted: `missing`, `not_acquired` or `upgrade` (below the quality cutoff). */
    @SerialName("wanted_reason") val wantedReason: String? = null,
    val watch: WatchState? = null,
)

@Serializable
data class EpisodeHit(
    /** Unix seconds, UTC. */
    @SerialName("airs_at") val airsAt: Long? = null,
    /** False when the provider only gave a date: show the date without a time. */
    @SerialName("airs_time_known") val airsTimeKnown: Boolean = false,
    /**
     * Artwork kinds available from `/api/artwork/{item_id}/{kind}`: `still`, the
     * episode's picture. The server chooses it: the provider's if there is one,
     * otherwise a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    @SerialName("item_id") val itemId: Long,
    val number: EpisodeNumber? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
    @SerialName("series_id") val seriesId: Long,
    @SerialName("series_title") val seriesTitle: String,
)

@Serializable
data class EpisodeList(
    /** Over this view's entries. */
    val counts: StateCounts,
    val entries: List<Episode> = emptyList(),
    val gaps: Long,
    /** Index into `entries`. */
    @SerialName("up_next") val upNext: Long? = null,
    val view: String? = null,
)

/** An episode's number in one ordering or playlist. Formatting is up to the client. */
@Serializable
data class EpisodeNumber(
    val number: Long? = null,
    val season: Long? = null,
    val style: NumberStyle,
)

@Serializable
data class EpisodeRef(
    /** Unix seconds, UTC. */
    @SerialName("airs_at") val airsAt: Long? = null,
    /** False when the provider only gave a date: show the date without a time. */
    @SerialName("airs_time_known") val airsTimeKnown: Boolean = false,
    /**
     * Artwork kinds available from `/api/artwork/{item_id}/{kind}`: `still`, the
     * episode's picture. The server chooses it: the provider's if there is one,
     * otherwise a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    @SerialName("item_id") val itemId: Long,
    val number: EpisodeNumber? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
)

/** Error body for every non-2xx response. */
@Serializable
data class ErrorBody(
    val error: String,
)

@Serializable
data class EventRecord(
    @SerialName("created_at") val createdAt: Long,
    /** Increasing, never reused: a consumer's cursor. */
    val id: Long,
    val payload: JsonObject,
    val type: String,
)

@Serializable
data class EventsPage(
    /** Pass as `after` next time. */
    val cursor: Long,
    val events: List<EventRecord> = emptyList(),
)

@Serializable
data class ExternalId(
    @SerialName("is_primary") val isPrimary: Boolean = false,
    val provider: String,
    val value: String,
)

@Serializable
data class ExternalStatus(
    val detail: JsonObject? = null,
    @SerialName("expires_at") val expiresAt: Long,
    val source: String,
    /** Past its expiry: the tool stopped reporting. */
    val stale: Boolean = false,
    val status: String,
    @SerialName("updated_at") val updatedAt: Long,
)

@Serializable
@JvmInline
value class Extra(val value: String) {
    companion object {
        val Sample = Extra("sample")
        val Trailer = Extra("trailer")
        val Featurette = Extra("featurette")
        val BehindTheScenes = Extra("behind_the_scenes")
        val DeletedScene = Extra("deleted_scene")
        val Interview = Extra("interview")
        val Short = Extra("short")
        val Other = Extra("other")
    }
}

@Serializable
data class FileCounts(
    val ignored: Long,
    val missing: Long,
    val present: Long,
)

@Serializable
data class FileReview(
    val candidates: List<Candidate> = emptyList(),
    @SerialName("duration_seconds") val durationSeconds: Double? = null,
    @SerialName("file_id") val fileId: Long,
    /** What the folder name says, for searching. */
    @SerialName("folder_hints") val folderHints: Hints? = null,
    /**
     * The series or movie folder the template puts it in; for a loose movie, the
     * file itself. None for a TV file outside any series folder.
     */
    @SerialName("item_folder") val itemFolder: String? = null,
    /** `tv` or `movies`. */
    val kind: String,
    /** The series or movie its folder belongs to, if known. */
    @SerialName("parent_id") val parentId: Long? = null,
    @SerialName("parent_title") val parentTitle: String? = null,
    val parsed: Parsed,
    @SerialName("path_pieces") val pathPieces: List<PathPiece> = emptyList(),
    @SerialName("relative_path") val relativePath: String,
    @SerialName("root_id") val rootId: Long,
    @SerialName("root_path") val rootPath: String,
)

@Serializable
data class FixFolderIds(
    /** Rename it even on a read-only root. */
    val force: Boolean? = null,
    @SerialName("root_id") val rootId: Long,
)

@Serializable
data class FolderEntry(
    /** Videos in it and below it whose file has gone missing. */
    val missing: Long,
    val name: String,
    /** Relative to the library's roots; pass it to browse this folder. */
    val path: String,
    /** Videos present in it and everything below it. */
    val videos: Long,
    /** Of those, the ones the viewer has watched. */
    val watched: Long,
)

@Serializable
data class FolderListing(
    /** Subfolder names, hidden ones left out. */
    val folders: List<String> = emptyList(),
    val parent: String? = null,
    val path: String,
)

@Serializable
data class FolderPage(
    val folders: List<FolderEntry> = emptyList(),
    @SerialName("library_id") val libraryId: Long,
    /** This folder, relative to the library's roots. Empty at the top. */
    val path: String,
    val videos: List<FolderVideo> = emptyList(),
)

@Serializable
data class FolderProposal(
    /** Item folders it finds, e.g. "Night Harbor (2019)". */
    val examples: List<String> = emptyList(),
    val files: Long,
    /** Files the template groups into an item folder. */
    val grouped: Long,
    /** Why this template, in a sentence. */
    val reason: String,
    val template: String,
    /** The folder, resolved. */
    val path: String,
    val pieces: List<TemplatePiece> = emptyList(),
)

@Serializable
data class FolderRename(
    val from: String,
    @SerialName("root_id") val rootId: Long,
    val to: String,
)

@Serializable
data class FolderReview(
    val candidates: List<Candidate> = emptyList(),
    val files: Long,
    @SerialName("folder_path") val folderPath: String,
    val hints: Hints,
    /** `tv` or `movies`. */
    val kind: String,
    @SerialName("path_pieces") val pathPieces: List<PathPiece> = emptyList(),
    @SerialName("root_id") val rootId: Long,
    @SerialName("root_path") val rootPath: String,
)

@Serializable
data class FolderVideo(
    /**
     * Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop,
     * and `thumbnail`, a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    val availability: Availability,
    val genres: List<String> = emptyList(),
    val id: Long,
    /** The library it belongs to; null only if it predates having one. */
    @SerialName("library_id") val libraryId: Long? = null,
    val monitored: Boolean = false,
    /** Release, Unix seconds. */
    val release: Long? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
    /** Present files: versions and editions. */
    val versions: Long,
    val wanted: Boolean = false,
    val watch: WatchState? = null,
    val year: Long? = null,
    @SerialName("file_name") val fileName: String,
)

@Serializable
data class GuestDevice(
    /** Shown in Account, where it can be signed out. */
    @SerialName("device_name") val deviceName: String,
)

@Serializable
data class Health(
    val status: String,
    val version: String,
)

@Serializable
data class Hints(
    val genre: String? = null,
    /** (provider, id) pairs found in the path. */
    val ids: List<List<String>> = emptyList(),
    val season: Long? = null,
    /** The item's name from its folder, without year or IDs. */
    val title: String,
    val year: Long? = null,
)

@Serializable
data class Home(
    val attention: Attention,
    @SerialName("coming_up") val comingUp: List<ComingUp> = emptyList(),
    /** The most recently watched series' next episode. */
    val hero: ContinueItem? = null,
    @SerialName("keep_watching") val keepWatching: List<ContinueItem> = emptyList(),
    val movies: List<MovieSummary> = emptyList(),
    /** Episodes that aired in the last two days, oldest first. */
    @SerialName("recently_aired") val recentlyAired: List<ComingUp> = emptyList(),
    val shows: List<SeriesSummary> = emptyList(),
)

@Serializable
data class IdentifyFolder(
    @SerialName("folder_path") val folderPath: String,
    /** The provider's ID for the series or movie. */
    @SerialName("provider_id") val providerId: String,
    @SerialName("root_id") val rootId: Long,
)

@Serializable
data class IgnoreFolder(
    @SerialName("folder_path") val folderPath: String,
    @SerialName("root_id") val rootId: Long,
)

@Serializable
data class IgnoredFolder(
    /** Files in it, all ignored. */
    val files: Long,
    @SerialName("folder_path") val folderPath: String,
    val id: Long,
    @SerialName("ignored_at") val ignoredAt: Long,
    @SerialName("root_id") val rootId: Long,
    @SerialName("root_path") val rootPath: String,
)

@Serializable
data class ImportReport(
    /** True when the series or movie was new. */
    val created: Boolean = false,
    @SerialName("episodes_added") val episodesAdded: Long,
    /** Dropped by the provider but kept: they have files or watch history. */
    @SerialName("episodes_flagged") val episodesFlagged: Long,
    @SerialName("episodes_removed") val episodesRemoved: Long,
    @SerialName("episodes_updated") val episodesUpdated: Long,
    @SerialName("item_id") val itemId: Long,
)

@Serializable
data class ImportRequest(
    /** Report where it would go without touching anything. */
    @SerialName("dry_run") val dryRun: Boolean? = null,
    /**
     * Episodes of that series, e.g. `["S01E02", "S01E03"]` for a double episode.
     * Without them, a series' episodes are read from the file name.
     */
    val episodes: List<String>? = null,
    /** The episode, movie or series: `item:<id>` or `<provider>:<id>`. */
    val item: String,
    val mode: TransferMode? = null,
    /** Ordering the episode numbers are in; default the series' default. */
    val ordering: String? = null,
    /** Absolute path of the file on the server. */
    val path: String,
    /** Remove the items' existing files (an upgrade). Off by default. */
    val replace: Boolean? = null,
)

@Serializable
data class ImportResult(
    @SerialName("file_id") val fileId: Long,
    val items: List<Long> = emptyList(),
    val note: String,
    val path: String,
    /** Files of the same items removed because `replace` was asked. */
    val replaced: List<String> = emptyList(),
    @SerialName("root_id") val rootId: Long,
    val sidecars: List<String> = emptyList(),
)

@Serializable
data class IssuerFound(
    /** The issuer as the provider names itself; save this one. */
    val issuer: String,
)

@Serializable
data class IssuerTest(
    val issuer: String,
)

/** Fields people can edit. Absent fields are left alone. */
@Serializable
data class ItemEdit(
    /** Lock the edited fields so metadata refreshes never overwrite them. */
    val lock: Boolean? = null,
    @SerialName("original_title") val originalTitle: String? = null,
    val overview: String? = null,
    @SerialName("sort_title") val sortTitle: String? = null,
    val title: String? = null,
    val year: Long? = null,
)

@Serializable
data class ItemRef(
    val id: Long,
    /** `series`, `episode` or `movie`. */
    val kind: String,
    @SerialName("library_id") val libraryId: Long,
    /** Episodes: the number in the default ordering, or else the first one. */
    val number: EpisodeNumber? = null,
    /** Episodes: their series. */
    @SerialName("series_id") val seriesId: Long? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    val title: String,
)

@Serializable
data class ItemView(
    @SerialName("airs_at") val airsAt: Long? = null,
    /** Episodes and movies only. */
    val availability: Availability? = null,
    @SerialName("external_ids") val externalIds: List<ExternalId> = emptyList(),
    val files: List<LinkedFile> = emptyList(),
    val id: Long,
    /** `series`, `episode` or `movie`. */
    val kind: String,
    val monitored: Boolean? = null,
    /** Its number in every ordering it appears in. */
    val numbers: List<OrderingNumber> = emptyList(),
    @SerialName("parent_id") val parentId: Long? = null,
    @SerialName("parent_title") val parentTitle: String? = null,
    val statuses: List<ExternalStatus> = emptyList(),
    val title: String,
    @SerialName("wanted_reason") val wantedReason: String? = null,
    val year: Long? = null,
)

@Serializable
data class ItemWatch(
    @SerialName("item_id") val itemId: Long,
    @SerialName("last_played_at") val lastPlayedAt: Long? = null,
    @SerialName("play_count") val playCount: Long,
    val played: Boolean = false,
    @SerialName("position_seconds") val positionSeconds: Double,
)

/** How a provider signs people in. */
@Serializable
@JvmInline
value class Kind(val value: String) {
    companion object {
        val Oidc = Kind("oidc")
        val Github = Kind("github")
        val Discord = Kind("discord")
        val Oauth2 = Kind("oauth2")
    }
}

@Serializable
@JvmInline
value class Level(val value: String) {
    companion object {
        val Good = Level("good")
        val Info = Level("info")
        val Warning = Level("warning")
        val Problem = Level("problem")
    }
}

@Serializable
data class Library(
    val id: Long,
    /** `tv`, `movies` or `folders`. */
    val kind: String,
    /** The last metadata refresh of the whole library. */
    @SerialName("last_refresh") val lastRefresh: ScanRun? = null,
    val name: String,
    /** Set while the library's metadata is refreshed; filled in by whoever runs tasks. */
    val refreshing: RefreshProgress? = null,
    val roots: List<Root> = emptyList(),
)

@Serializable
data class LinkFile(
    /** Episodes (several for a multi-episode file) or a movie. */
    @SerialName("item_ids") val itemIds: List<Long> = emptyList(),
    val pin: Boolean? = null,
)

@Serializable
data class LinkedFile(
    val confidence: Double,
    val container: String? = null,
    val edition: String? = null,
    @SerialName("file_id") val fileId: Long,
    @SerialName("hdr_format") val hdrFormat: String? = null,
    val height: Long? = null,
    @SerialName("last_seen") val lastSeen: Long,
    /** How it was matched: `path_id`, `nfo`, `parser`, `ai` or `user`. */
    @SerialName("link_source") val linkSource: String,
    /** For movies split over several files. */
    val part: Long? = null,
    val pinned: Boolean = false,
    @SerialName("relative_path") val relativePath: String,
    @SerialName("root_path") val rootPath: String,
    val size: Long,
    /** `present`, `missing` or `ignored`. */
    val status: String,
    @SerialName("video_codec") val videoCodec: String? = null,
    val width: Long? = null,
)

@Serializable
data class Location(
    /**
     * The path with the provider IDs in the folder's name corrected, when they
     * disagree with the series' own (a wrong `[tmdb-…]` tag, say).
     */
    @SerialName("corrected_path") val correctedPath: String? = null,
    @SerialName("file_count") val fileCount: Long,
    @SerialName("is_primary") val isPrimary: Boolean = false,
    /** Whether the root folder is reachable right now. */
    val online: Boolean = false,
    /** Relative to the root. */
    val path: String,
    @SerialName("root_id") val rootId: Long,
    @SerialName("root_path") val rootPath: String,
    @SerialName("write_policy") val writePolicy: String,
)

@Serializable
data class Locks(
    /** Fields refreshes must not overwrite; empty unlocks all. */
    val fields: List<String> = emptyList(),
)

/** A linked account, as its user and admins see it. */
@Serializable
data class Login(
    /** The account's email at the provider, as last seen. */
    val email: String? = null,
    val id: Long,
    @SerialName("last_used_at") val lastUsedAt: Long? = null,
    @SerialName("linked_at") val linkedAt: Long,
    @SerialName("provider_key") val providerKey: String,
    @SerialName("provider_name") val providerName: String,
)

/** A user as admins see them. */
@Serializable
data class ManagedUser(
    @SerialName("created_at") val createdAt: Long,
    val email: String? = null,
    val id: Long,
    /** Hides what the user watches from other users, not from admins. */
    val incognito: Boolean = false,
    @SerialName("is_admin") val isAdmin: Boolean = false,
    /** A throwaway account from "Continue as guest" ([`crate::guests`]). */
    @SerialName("is_guest") val isGuest: Boolean = false,
    val name: String,
    val status: UserStatus,
    @SerialName("has_password") val hasPassword: Boolean = false,
    /** Accounts at sign-in providers. */
    val logins: List<Login> = emptyList(),
    /** The provider whose groups decide whether they're an admin, if any. */
    @SerialName("role_from") val roleFrom: String? = null,
)

@Serializable
data class MatchOrdering(
    /** `ordering:<id>` of this series, or null to try the default first. */
    val view: String? = null,
)

@Serializable
data class MeUpdate(
    @SerialName("current_password") val currentPassword: String? = null,
    /** Empty to remove it. */
    val email: String? = null,
    /** Hide what you watch from other users (not from admins), from now on. */
    val incognito: Boolean? = null,
    val name: String? = null,
    /** A new password; needs `current_password` if there is one. */
    val password: String? = null,
)

/** A token for media requests, which can't send a header. */
@Serializable
data class MediaToken(
    @SerialName("expires_at") val expiresAt: Long,
    /** Sent as `?media_token=` on artwork, streams and subtitles only. */
    val token: String,
)

@Serializable
@JvmInline
value class Mode(val value: String) {
    companion object {
        val Direct = Mode("direct")
        val Remux = Mode("remux")
        val Transcode = Mode("transcode")
    }
}

/** Monitoring settings. Leave a field out to keep it; send null to clear it. */
@Serializable
data class Monitoring(
    /** A quality profile ID, or null for none. */
    @SerialName("quality_profile_id") val qualityProfileId: Long? = null,
    /** Series only: `playlist:<id>` that wanted follows, or null for all episodes. */
    @SerialName("wanted_follows") val wantedFollows: String? = null,
)

@Serializable
data class MovePlan(
    val applied: Boolean = false,
    val moves: List<PlannedMove> = emptyList(),
    /** Files left where they are, and why. */
    val skipped: List<String> = emptyList(),
)

@Serializable
data class MoveSeries(
    @SerialName("dry_run") val dryRun: Boolean? = null,
    @SerialName("root_id") val rootId: Long,
)

@Serializable
data class MovieDetail(
    /**
     * Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop,
     * and `thumbnail`, a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    val availability: Availability,
    val genres: List<String> = emptyList(),
    val id: Long,
    /** The library it belongs to; null only if it predates having one. */
    @SerialName("library_id") val libraryId: Long? = null,
    val monitored: Boolean = false,
    /** Release, Unix seconds. */
    val release: Long? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
    /** Present files: versions and editions. */
    val versions: Long,
    val wanted: Boolean = false,
    val watch: WatchState? = null,
    val year: Long? = null,
    @SerialName("external_ids") val externalIds: List<ExternalId> = emptyList(),
    val files: List<LinkedFile> = emptyList(),
    @SerialName("locked_fields") val lockedFields: List<String> = emptyList(),
    @SerialName("original_title") val originalTitle: String? = null,
    val overview: String? = null,
    @SerialName("primary_provider") val primaryProvider: String? = null,
    @SerialName("refresh_error") val refreshError: String? = null,
    @SerialName("refreshed_at") val refreshedAt: Long? = null,
)

@Serializable
data class MovieSummary(
    /**
     * Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop,
     * and `thumbnail`, a frame from the video.
     */
    val artwork: List<String> = emptyList(),
    val availability: Availability,
    val genres: List<String> = emptyList(),
    val id: Long,
    /** The library it belongs to; null only if it predates having one. */
    @SerialName("library_id") val libraryId: Long? = null,
    val monitored: Boolean = false,
    /** Release, Unix seconds. */
    val release: Long? = null,
    @SerialName("runtime_seconds") val runtimeSeconds: Long? = null,
    val title: String,
    /** Present files: versions and editions. */
    val versions: Long,
    val wanted: Boolean = false,
    val watch: WatchState? = null,
    val year: Long? = null,
)

/** How you sign in: a password, linked accounts, and providers to link. */
@Serializable
data class MySignIn(
    @SerialName("has_password") val hasPassword: Boolean = false,
    val logins: List<Login> = emptyList(),
    val providers: List<ProviderButton> = emptyList(),
)

@Serializable
data class NewApiKey(
    @SerialName("created_at") val createdAt: Long,
    val id: Long,
    @SerialName("last_used_at") val lastUsedAt: Long? = null,
    val name: String,
    /** The key's first characters, to recognise it. */
    val prefix: String,
    /** The key itself. Shown only now; store it in the tool that uses it. */
    val secret: String,
)

@Serializable
data class NewUser(
    @SerialName("is_admin") val isAdmin: Boolean? = null,
    val name: String,
    val password: String,
)

@Serializable
data class NewWebhook(
    @SerialName("created_at") val createdAt: Long,
    /** The last event delivered. */
    val cursor: Long,
    val enabled: Boolean = false,
    /** Event types delivered; empty means all. */
    @SerialName("event_types") val eventTypes: List<String> = emptyList(),
    val failures: Long,
    val id: Long,
    @SerialName("last_delivered_at") val lastDeliveredAt: Long? = null,
    @SerialName("last_error") val lastError: String? = null,
    val name: String,
    @SerialName("next_attempt_at") val nextAttemptAt: Long,
    /** Events waiting to be delivered. */
    val pending: Long,
    val url: String,
    /** For verifying signatures. Shown only now. */
    val secret: String,
)

@Serializable
data class Note(
    val level: Level,
    val text: String,
)

@Serializable
@JvmInline
value class NumberStyle(val value: String) {
    companion object {
        val Season = NumberStyle("season")
        val Absolute = NumberStyle("absolute")
        val Playlist = NumberStyle("playlist")
    }
}

@Serializable
data class OrderingNumber(
    val kind: String,
    val number: Long? = null,
    val ordering: String,
    val season: Long? = null,
)

@Serializable
data class PairingAnswer(
    val approve: Boolean = false,
    val code: String,
)

@Serializable
data class PairingPoll(
    val secret: String,
)

@Serializable
data class PairingRequest(
    @SerialName("device_name") val deviceName: String,
)

@Serializable
data class PairingStart(
    /** Shown to the person approving, and in Account afterwards. */
    @SerialName("device_name") val deviceName: String,
)

@Serializable
data class PairingStarted(
    /** For the person to enter, as `XXXX-XXXX`. */
    val code: String,
    @SerialName("expires_in_seconds") val expiresInSeconds: Long,
    /**
     * Where to send the person on this server, with the code filled in. Open
     * it in the system browser, or show it as text and a QR code.
     */
    val path: String,
    @SerialName("poll_every_seconds") val pollEverySeconds: Long,
    /** For this device alone, to poll with. */
    val secret: String,
)

@Serializable
@JvmInline
value class PairingState(val value: String) {
    companion object {
        val Waiting = PairingState("waiting")
        val Approved = PairingState("approved")
        val Declined = PairingState("declined")
        val Expired = PairingState("expired")
    }
}

@Serializable
data class PairingStatus(
    val session: DeviceSession? = null,
    val state: PairingState,
)

@Serializable
data class Parsed(
    /** Absolute numbers, when the name has no season (typical for anime). */
    val absolute: List<Long> = emptyList(),
    /** Date-numbered episodes (daily shows). */
    val date: String? = null,
    val edition: String? = null,
    @SerialName("episode_title") val episodeTitle: String? = null,
    /** Episode numbers within `season`; several for multi-episode files. */
    val episodes: List<Long> = emptyList(),
    val extra: Extra? = null,
    /** Provider IDs written in the name: (provider, id). */
    val ids: List<List<String>> = emptyList(),
    /** For movies split over several files: cd1, part2… */
    val part: Long? = null,
    @SerialName("release_group") val releaseGroup: String? = null,
    val resolution: String? = null,
    val season: Long? = null,
    val title: String? = null,
    val version: Long? = null,
    val year: Long? = null,
)

@Serializable
data class PathPiece(
    /** The template level this path component sits at; `file` for the file name. */
    val role: String,
    val text: String,
)

/** A series or movie in the library someone is in, with every role they have there. */
@Serializable
data class PersonCredit(
    /** Artwork kinds available from `/api/artwork/{id}/{kind}`. */
    val artwork: List<String> = emptyList(),
    /** A series' regular: how many episodes they're in. */
    @SerialName("episode_count") val episodeCount: Long? = null,
    /** Episodes they guest in or worked on, in order. */
    val episodes: List<PersonEpisode> = emptyList(),
    @SerialName("item_id") val itemId: Long,
    /** `series` or `movie`. */
    val kind: String,
    @SerialName("library_id") val libraryId: Long,
    /** Characters and jobs, most prominent first, each once. */
    val roles: List<String> = emptyList(),
    val title: String,
    val year: Long? = null,
)

@Serializable
data class PersonDetail(
    val biography: String? = null,
    /** `YYYY-MM-DD`. */
    val birthday: String? = null,
    val birthplace: String? = null,
    /** Newest first. */
    val credits: List<PersonCredit> = emptyList(),
    val deathday: String? = null,
    /** The photo is at `/api/people/{id}/photo`. */
    @SerialName("has_photo") val hasPhoto: Boolean = false,
    val id: Long,
    @SerialName("known_for") val knownFor: String? = null,
    val name: String,
)

/** An episode someone is credited on. */
@Serializable
data class PersonEpisode(
    @SerialName("item_id") val itemId: Long,
    /** In the series' default ordering. */
    val number: EpisodeNumber? = null,
    val role: String? = null,
    val title: String,
)

@Serializable
@JvmInline
value class Phase(val value: String) {
    companion object {
        val Walking = Phase("walking")
        val Hashing = Phase("hashing")
        val Probing = Phase("probing")
        val Matching = Phase("matching")
    }
}

@Serializable
data class Placement(
    val note: String,
    /** Relative to the root; None when nothing is written to this root. */
    val path: String? = null,
)

@Serializable
data class PlacementExample(
    /** e.g. `S02E01`. */
    @SerialName("episode_label") val episodeLabel: String,
    @SerialName("episode_title") val episodeTitle: String,
    /** The name the file arrives with. */
    val filename: String,
    /** The series' stored folder on that root. */
    val location: String,
    val outcomes: List<PolicyPlacement> = emptyList(),
    @SerialName("root_path") val rootPath: String,
    @SerialName("series_title") val seriesTitle: String,
)

@Serializable
data class PlannedMove(
    @SerialName("file_id") val fileId: Long,
    val from: String,
    val to: String,
)

/** Where an episode or movie sits, for the player. */
@Serializable
data class PlayContext(
    @SerialName("item_id") val itemId: Long,
    /** `episode` or `movie`. */
    val kind: String,
    /** The next available episode there. */
    val next: EpisodeRef? = null,
    /** The episode's number in the user's ordering or playlist. */
    val number: EpisodeNumber? = null,
    @SerialName("series_id") val seriesId: Long? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    val title: String,
    val year: Long? = null,
)

@Serializable
data class PlayPlan(
    @SerialName("audio_stream") val audioStream: Long? = null,
    val context: PlayContext,
    /** The video encoder in use when transcoding: `software`, `vaapi`, `nvenc` or `qsv`. */
    val encoder: String,
    val mode: Mode,
    /**
     * The stream's time 0 is this far into the file. Zero when direct: seek
     * to `start_seconds` instead.
     */
    @SerialName("offset_seconds") val offsetSeconds: Double,
    /** What the HLS stream carries; none when playing directly. */
    val output: StreamOutput? = null,
    /** Why it isn't played directly. */
    val reasons: List<String> = emptyList(),
    /** The HLS session, to stop when done. */
    @SerialName("session_id") val sessionId: String? = null,
    @SerialName("start_seconds") val startSeconds: Double,
    val subtitle: String? = null,
    val subtitles: List<SubtitleSource> = emptyList(),
    /** The file itself (direct) or an HLS playlist. */
    val url: String,
    val version: Version,
    val watch: ItemWatch,
)

@Serializable
data class PlayRequest(
    /** An audio stream index; defaults to the file's default audio. */
    @SerialName("audio_stream") val audioStream: Long? = null,
    /** Which file; defaults to the best present one. */
    @SerialName("file_id") val fileId: Long? = null,
    /** Transcode down to this height (a quality choice), e.g. 720. */
    @SerialName("max_height") val maxHeight: Long? = null,
    /** `stream:<index>` or `sidecar:<id>`; none for no subtitles. */
    val subtitle: String? = null,
    val caps: ClientCaps? = null,
    /** Where to start; defaults to the resume position. */
    @SerialName("start_seconds") val startSeconds: Double? = null,
)

@Serializable
data class PlayedChanged(
    val changed: Long,
)

@Serializable
data class Playlist(
    /** Smart: the ordering it follows. */
    @SerialName("base_ordering_id") val baseOrderingId: Long? = null,
    /** Smart: episodes with any of these tags are left out. */
    @SerialName("exclude_tags") val excludeTags: List<String> = emptyList(),
    val id: Long,
    /** Manual: the episodes, in order. */
    val items: List<Long> = emptyList(),
    /** `smart` or `manual`. */
    val kind: String,
    val name: String,
    @SerialName("owner_user_id") val ownerUserId: Long? = null,
    @SerialName("series_id") val seriesId: Long,
    /** Everyone sees it; otherwise only its owner. */
    val shared: Boolean = false,
)

@Serializable
data class PlaylistInput(
    /** Smart: an ordering of the series. */
    @SerialName("base_ordering_id") val baseOrderingId: Long? = null,
    @SerialName("exclude_tags") val excludeTags: List<String>? = null,
    /** Manual: episode IDs of one series, in order. */
    val items: List<Long>? = null,
    /** `smart` or `manual` (create only). */
    val kind: String? = null,
    val name: String? = null,
    /** Make it everyone's (admins only; create only). */
    val shared: Boolean? = null,
)

@Serializable
data class PolicyPlacement(
    val note: String,
    /** Relative to the root; None when nothing is written to this root. */
    val path: String? = null,
    /** `managed`, `adaptive` or `read_only`. */
    val policy: String,
)

@Serializable
data class PostedCallback(
    val code: String? = null,
    val error: String? = null,
    @SerialName("error_description") val errorDescription: String? = null,
    val state: String? = null,
    /** Apple's, on someone's first sign-in: their name and email as JSON. */
    val user: String? = null,
)

@Serializable
data class Progress(
    /** What the player knows; otherwise the file's probed duration is used. */
    @SerialName("duration_seconds") val durationSeconds: Double? = null,
    @SerialName("position_seconds") val positionSeconds: Double,
)

@Serializable
data class Proposal(
    /** Item folders it finds, e.g. "Night Harbor (2019)". */
    val examples: List<String> = emptyList(),
    val files: Long,
    /** Files the template groups into an item folder. */
    val grouped: Long,
    /** Why this template, in a sentence. */
    val reason: String,
    val template: String,
)

@Serializable
data class ProposeTemplate(
    /** `tv`, `movies` or `folders`. */
    val kind: String,
    val path: String,
)

/** A button on the login page. */
@Serializable
data class ProviderButton(
    val key: String,
    val name: String,
)

/** Settings to try, saved or not. */
@Serializable
data class ProviderDraft(
    /** The provider being edited: its secret is used when none is given. */
    val existing: String? = null,
    val provider: ProviderInput,
)

@Serializable
data class ProviderInput(
    @SerialName("admin_group") val adminGroup: String? = null,
    @SerialName("allowed_groups") val allowedGroups: List<String>? = null,
    /** Plain OAuth2 only, as are the fields below. */
    @SerialName("authorize_url") val authorizeUrl: String? = null,
    @SerialName("client_id") val clientId: String,
    /** Leave out to keep the current one. */
    @SerialName("client_secret") val clientSecret: String? = null,
    @SerialName("email_field") val emailField: String? = null,
    val enabled: Boolean? = null,
    @SerialName("groups_claim") val groupsClaim: String? = null,
    @SerialName("id_field") val idField: String? = null,
    /** OIDC only. */
    val issuer: String? = null,
    /** Defaults to one made from the name. */
    val key: String? = null,
    @SerialName("key_id") val keyId: String? = null,
    val kind: Kind? = null,
    val name: String,
    @SerialName("name_field") val nameField: String? = null,
    @SerialName("new_users") val newUsers: UserStatus? = null,
    /** Picked by name in the form, e.g. `keycloak`. */
    val preset: String? = null,
    @SerialName("redirect_url") val redirectUrl: String? = null,
    val scopes: String? = null,
    /**
     * Sign in with Apple only, as is `key_id`; the private key goes in
     * `client_secret`.
     */
    @SerialName("team_id") val teamId: String? = null,
    @SerialName("token_url") val tokenUrl: String? = null,
    @SerialName("userinfo_url") val userinfoUrl: String? = null,
    @SerialName("verified_field") val verifiedField: String? = null,
)

/** A provider as admins see it: never the secret. */
@Serializable
data class ProviderView(
    @SerialName("admin_group") val adminGroup: String? = null,
    @SerialName("allowed_groups") val allowedGroups: List<String> = emptyList(),
    @SerialName("authorize_url") val authorizeUrl: String? = null,
    @SerialName("client_id") val clientId: String,
    @SerialName("email_field") val emailField: String? = null,
    val enabled: Boolean = false,
    /** Set for providers from environment variables. */
    val env: EnvInfo? = null,
    @SerialName("groups_claim") val groupsClaim: String,
    @SerialName("has_secret") val hasSecret: Boolean = false,
    @SerialName("id_field") val idField: String? = null,
    val issuer: String,
    val key: String,
    @SerialName("key_id") val keyId: String? = null,
    val kind: Kind,
    val name: String,
    @SerialName("name_field") val nameField: String? = null,
    @SerialName("new_users") val newUsers: UserStatus,
    val preset: String? = null,
    @SerialName("redirect_url") val redirectUrl: String? = null,
    val scopes: String,
    /** People who can sign in only with this provider. */
    @SerialName("sole_users") val soleUsers: Long,
    @SerialName("team_id") val teamId: String? = null,
    @SerialName("token_url") val tokenUrl: String? = null,
    @SerialName("userinfo_url") val userinfoUrl: String? = null,
    /** People with a linked account. */
    val users: Long,
    @SerialName("verified_field") val verifiedField: String? = null,
)

@Serializable
data class QualityProfile(
    val id: Long,
    val name: String,
)

/**
 * One movie in a format both Radarr list types read: "Radarr List" uses `id`
 * (TMDB), "StevenLu Custom" uses `title` and `imdb_id`.
 */
@Serializable
data class RadarrListItem(
    val id: Long? = null,
    @SerialName("imdb_id") val imdbId: String? = null,
    val title: String,
)

@Serializable
data class Rating(
    /** What a perfect score is: 10 or 100. */
    val scale: Double,
    /** `imdb`, `rotten_tomatoes`, `metacritic` or `tmdb`. */
    val source: String,
    val value: Double,
    val votes: Long? = null,
)

/** A library refresh in progress. */
@Serializable
data class RefreshProgress(
    /** Series and movies refreshed so far, of `total`. */
    val done: Long,
    /** Another refresh is due when this one finishes. */
    @SerialName("queued_again") val queuedAgain: Boolean = false,
    @SerialName("started_at") val startedAt: Long,
    val total: Long,
    val trigger: String,
)

@Serializable
data class RefreshStarted(
    /** False when a refresh was already running; one more runs after it. */
    val started: Boolean = false,
)

@Serializable
data class Rematch(
    /** The provider's ID for the right series. */
    @SerialName("provider_id") val providerId: String,
)

@Serializable
data class Removed(
    /** People who could only sign in with it, now turned off. */
    @SerialName("disabled_users") val disabledUsers: Long,
)

@Serializable
data class RenameLibrary(
    val name: String,
)

@Serializable
data class ReviewQueue(
    /** Files with pending candidates. */
    val files: List<FileReview> = emptyList(),
    val folders: List<FolderReview> = emptyList(),
    /** Files with nothing to propose: no numbers that fit, or no identified folder. */
    val unmatched: List<FileReview> = emptyList(),
)

@Serializable
data class Root(
    val files: FileCounts,
    val id: Long,
    @SerialName("is_import_target") val isImportTarget: Boolean = false,
    @SerialName("last_scan") val lastScan: ScanRun? = null,
    @SerialName("layout_template") val layoutTemplate: String,
    @SerialName("library_id") val libraryId: Long,
    /** When a scan last found it unreachable; cleared by a successful scan. */
    @SerialName("offline_since") val offlineSince: Long? = null,
    /** Whether the folder is reachable right now. */
    val online: Boolean = false,
    val path: String,
    /** Set while a scan runs; filled in by whoever runs scans. */
    val scanning: ScanProgress? = null,
    /** The template split for display. */
    @SerialName("template_pieces") val templatePieces: List<TemplatePiece> = emptyList(),
    /** `managed`, `adaptive` or `read_only`. */
    @SerialName("write_policy") val writePolicy: String,
)

@Serializable
data class ScanProgress(
    /**
     * Files found while walking (with no total), files hashed or probed,
     * or folders matched so far.
     */
    val done: Long,
    val phase: Phase,
    /** Another scan is due when this one finishes. */
    @SerialName("queued_again") val queuedAgain: Boolean = false,
    @SerialName("started_at") val startedAt: Long,
    val total: Long,
    val trigger: String,
)

/** A finished or running task: a root's scan or a library's metadata refresh. */
@Serializable
data class ScanRun(
    val error: String? = null,
    @SerialName("finished_at") val finishedAt: Long? = null,
    val id: Long,
    val report: JsonObject? = null,
    @SerialName("started_at") val startedAt: Long,
    /** `running`, `ok` or `failed`. */
    val state: String,
    /** `added`, `manual`, `scheduled` or `watch`. */
    val trigger: String,
)

@Serializable
data class ScanStarted(
    /** False when a scan was already running; one more runs after it. */
    val started: Boolean = false,
)

@Serializable
data class SearchResult(
    val id: String,
    @SerialName("original_title") val originalTitle: String? = null,
    val overview: String? = null,
    val provider: String,
    val title: String,
    val year: Long? = null,
)

@Serializable
data class SearchResults(
    val episodes: List<EpisodeHit> = emptyList(),
    val movies: List<MovieSummary> = emptyList(),
    val series: List<SeriesSummary> = emptyList(),
)

@Serializable
data class SeriesDetail(
    /** Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop. */
    val artwork: List<String> = emptyList(),
    val counts: StateCounts,
    val genres: List<String> = emptyList(),
    val id: Long,
    /** The library it belongs to; null only if it predates having one. */
    @SerialName("library_id") val libraryId: Long? = null,
    /** Available, unplayed episodes whose file arrived in the last 7 days. */
    @SerialName("new_episodes") val newEpisodes: Long,
    @SerialName("next_airing") val nextAiring: EpisodeRef? = null,
    /** `continuing`, `ended` or `cancelled`. */
    @SerialName("series_status") val seriesStatus: String? = null,
    val title: String,
    val year: Long? = null,
    @SerialName("external_ids") val externalIds: List<ExternalId> = emptyList(),
    val locations: List<Location> = emptyList(),
    /** Fields metadata refreshes never overwrite. */
    @SerialName("locked_fields") val lockedFields: List<String> = emptyList(),
    /** The view filenames are numbered in, if set; otherwise the default first. */
    @SerialName("match_view") val matchView: String? = null,
    /** The series' own setting; episodes inherit it unless set themselves. */
    val monitored: Boolean = false,
    val overview: String? = null,
    @SerialName("primary_provider") val primaryProvider: String? = null,
    @SerialName("quality_profile_id") val qualityProfileId: Long? = null,
    @SerialName("quality_profiles") val qualityProfiles: List<QualityProfile> = emptyList(),
    @SerialName("refresh_error") val refreshError: String? = null,
    @SerialName("refreshed_at") val refreshedAt: Long? = null,
    /** The current user's view, or the default ordering. */
    @SerialName("selected_view") val selectedView: String? = null,
    val views: List<View> = emptyList(),
    /** The playlist wanted follows (`playlist:<id>`), or none for all episodes. */
    @SerialName("wanted_follows") val wantedFollows: String? = null,
)

@Serializable
data class SeriesSummary(
    /** Artwork kinds available from `/api/artwork/{id}/{kind}`: poster, backdrop. */
    val artwork: List<String> = emptyList(),
    val counts: StateCounts,
    val genres: List<String> = emptyList(),
    val id: Long,
    /** The library it belongs to; null only if it predates having one. */
    @SerialName("library_id") val libraryId: Long? = null,
    /** Available, unplayed episodes whose file arrived in the last 7 days. */
    @SerialName("new_episodes") val newEpisodes: Long,
    @SerialName("next_airing") val nextAiring: EpisodeRef? = null,
    /** `continuing`, `ended` or `cancelled`. */
    @SerialName("series_status") val seriesStatus: String? = null,
    val title: String,
    val year: Long? = null,
)

@Serializable
data class ServerInfo(
    /** Apps may offer "Continue as guest" (`POST /auth/device/guest`). */
    @SerialName("guest_access") val guestAccess: Boolean = false,
    /** Apps older than this should ask to be updated. */
    @SerialName("min_app_version") val minAppVersion: String,
    /** What to call this server; `CABINET_SERVER_NAME`, or "Media Cabinet". */
    val name: String,
    val version: String,
)

@Serializable
data class SetMonitor(
    /** `null` inherits from the parent (an episode from its series). */
    val monitored: Boolean? = null,
)

@Serializable
data class SetPlayed(
    /** For a series: only these of its episodes. */
    val episodes: List<Long>? = null,
    val played: Boolean = false,
    /** For a series: only this season (an ordering season's ID). */
    @SerialName("season_id") val seasonId: Long? = null,
)

@Serializable
data class SetView(
    /** `ordering:<id>` or `playlist:<id>` of this series. */
    val view: String,
)

/** Sign-in providers and the password switch, for Manage → Users. */
@Serializable
data class SignInSettings(
    /** A provider's redirect URL is this plus `{key}/callback`. */
    @SerialName("callback_base") val callbackBase: String,
    /** Whether people may continue as a guest. */
    @SerialName("guest_access") val guestAccess: Boolean = false,
    /** `CABINET_GUEST_ACCESS` is set, so the switch is read-only. */
    @SerialName("guest_access_from_env") val guestAccessFromEnv: Boolean = false,
    /** Guests there are now. */
    val guests: Long,
    @SerialName("password_sign_in") val passwordSignIn: Boolean = false,
    /** `CABINET_PASSWORD_SIGN_IN` is set, so the switch is read-only. */
    @SerialName("password_sign_in_from_env") val passwordSignInFromEnv: Boolean = false,
    val providers: List<ProviderView> = emptyList(),
)

/** One series in Sonarr's "Custom List" format. */
@Serializable
data class SonarrListItem(
    val imdbId: String? = null,
    val title: String,
    val tmdbId: Long? = null,
    val tvdbId: Long? = null,
)

@Serializable
data class StateCounts(
    val available: Long,
    val missing: Long,
    @SerialName("not_acquired") val notAcquired: Long,
    val total: Long,
    val unknown: Long,
    val upcoming: Long,
    val wanted: Long,
)

@Serializable
data class StatusChange(
    val status: UserStatus,
)

@Serializable
data class StatusUpdate(
    /** JSON detail, e.g. `{"progress": 0.4, "eta_seconds": 540}`. */
    val detail: JsonObject? = null,
    /** Seconds until the status counts as stale. Default one hour, at most a week. */
    @SerialName("expires_in_seconds") val expiresInSeconds: Long? = null,
    /**
     * Free text. Well-known values the UI styles: searching, queued, downloading,
     * importing, failed.
     */
    val status: String,
)

/** What the last probe found in a file, as stored, with the sidecars next to it. */
@Serializable
data class StoredProbe(
    /** ffprobe's format name, e.g. `matroska,webm`. */
    val container: String? = null,
    @SerialName("duration_seconds") val durationSeconds: Double? = null,
    @SerialName("file_id") val fileId: Long,
    @SerialName("probe_error") val probeError: String? = null,
    /** Null until probed; reset when the content changes. */
    @SerialName("probed_at") val probedAt: Long? = null,
    val sidecars: List<StoredSidecar> = emptyList(),
    /** In the file's own order. */
    val streams: List<StoredStream> = emptyList(),
)

@Serializable
data class StoredSidecar(
    val id: Long,
    @SerialName("is_forced") val isForced: Boolean = false,
    @SerialName("is_hearing_impaired") val isHearingImpaired: Boolean = false,
    /** `subtitle`, `nfo` or `artwork`. */
    val kind: String,
    val language: String? = null,
    /** `root` (next to the video) or `cache`. */
    val location: String,
    @SerialName("relative_path") val relativePath: String,
)

@Serializable
data class StoredStream(
    /** Bits per second. */
    val bitrate: Long? = null,
    val channels: Long? = null,
    val codec: String? = null,
    @SerialName("hdr_format") val hdrFormat: String? = null,
    val height: Long? = null,
    val index: Long,
    @SerialName("is_default") val isDefault: Boolean = false,
    @SerialName("is_forced") val isForced: Boolean = false,
    /** `video`, `audio` or `subtitle`. */
    val kind: String,
    val language: String? = null,
    val width: Long? = null,
)

/** What an HLS stream is made of, for the player to show beside the source. */
@Serializable
data class StreamOutput(
    /** Set when the audio is converted. */
    @SerialName("audio_bitrate_kbps") val audioBitrateKbps: Int? = null,
    @SerialName("audio_channels") val audioChannels: Long? = null,
    /** The audio codec sent: the track's own when copied, otherwise `aac`. */
    @SerialName("audio_codec") val audioCodec: String? = null,
    /** A picture subtitle drawn onto the video. */
    @SerialName("burned_subtitle") val burnedSubtitle: Boolean = false,
    /** The height the video is encoded at; the file's own when copied. */
    val height: Long? = null,
    /** The length of the first segments, when shorter. */
    @SerialName("initial_segment_seconds") val initialSegmentSeconds: Int? = null,
    /** The encoder's bitrate ceiling, in kbit/s. */
    @SerialName("max_bitrate_kbps") val maxBitrateKbps: Int? = null,
    @SerialName("segment_seconds") val segmentSeconds: Int,
    val tonemap: Boolean = false,
    /** The video codec sent: the file's own when copied, otherwise `h264`. */
    @SerialName("video_codec") val videoCodec: String? = null,
    /** The H.264 encoder (`libx264`, `h264_nvenc`…); none when the video is copied. */
    @SerialName("video_encoder") val videoEncoder: String? = null,
)

/** How an HLS session is doing on the server, for the player's stats. */
@Serializable
data class StreamStats(
    /** The output's average bitrate so far, from the segments written, in kbit/s. */
    @SerialName("bitrate_kbps") val bitrateKbps: Double? = null,
    /** The ffmpeg command line, for admins. */
    val command: String? = null,
    /** How much of the stream is written, in the stream's time. */
    @SerialName("converted_seconds") val convertedSeconds: Double,
    /** ffmpeg reached the end of the file. */
    val finished: Boolean = false,
    /** How long ffmpeg took to write its first segment. */
    @SerialName("first_segment_seconds") val firstSegmentSeconds: Double? = null,
    /** Frames written per second. */
    val fps: Double? = null,
    /** The end of ffmpeg's error log, for admins. */
    val log: String? = null,
    @SerialName("running_seconds") val runningSeconds: Double,
    /** The highest segment the player has asked for. */
    @SerialName("segments_requested") val segmentsRequested: Long,
    @SerialName("segments_written") val segmentsWritten: Long,
    /** ffmpeg's speed as a multiple of real time; above 1 keeps ahead. */
    val speed: Double? = null,
    /** Paused for being far enough ahead of the player. */
    val throttled: Boolean = false,
)

@Serializable
data class SubtitleSource(
    val forced: Boolean = false,
    @SerialName("hearing_impaired") val hearingImpaired: Boolean = false,
    /** `stream:<index>` or `sidecar:<id>`. */
    val id: String,
    val language: String? = null,
    /** WebVTT, timed to the stream. None when burned into the video. */
    val url: String? = null,
)

@Serializable
data class SubtitleTrack(
    val codec: String? = null,
    val external: Boolean = false,
    val forced: Boolean = false,
    @SerialName("hearing_impaired") val hearingImpaired: Boolean = false,
    /** `stream:<index>` or `sidecar:<id>`. */
    val id: String,
    val language: String? = null,
    /** Text subtitles are sent as WebVTT; picture ones (PGS, DVD) are burned in. */
    val text: Boolean = false,
)

@Serializable
data class Tags(
    /** Who sets them, e.g. `animefillerlist`. Replaces that source's tags only. */
    val source: String,
    val tags: List<String> = emptyList(),
)

@Serializable
data class TaskDetail(
    val cancellable: Boolean = false,
    val error: String? = null,
    @SerialName("finished_at") val finishedAt: Long? = null,
    val id: Long,
    /** `scan`, `metadata_refresh` or `thumbnails`. */
    val kind: String,
    @SerialName("library_id") val libraryId: Long? = null,
    /** The library it works on, or its root's. */
    @SerialName("library_name") val libraryName: String? = null,
    /** While it has a place in the runner, including while it waits for its turn. */
    val progress: TaskProgress? = null,
    @SerialName("requested_at") val requestedAt: Long,
    @SerialName("root_id") val rootId: Long? = null,
    @SerialName("root_path") val rootPath: String? = null,
    @SerialName("started_at") val startedAt: Long? = null,
    /** `queued`, `running`, `ok`, `failed` or `cancelled`. */
    val state: String,
    /** What asked for it: `added`, `manual`, `scheduled` or `watch`. */
    val trigger: String,
    val log: List<TaskLogLine> = emptyList(),
    /** What it did, per kind. `failures` lists what went wrong, when there's a list. */
    val outcome: JsonObject? = null,
)

@Serializable
data class TaskLogLine(
    val at: Long,
    val fields: JsonObject? = null,
    /** `info`, `warn` or `error`. */
    val level: String,
    val message: String,
)

@Serializable
data class TaskPage(
    /** Newest first. */
    val finished: List<TaskSummary> = emptyList(),
    /** There are more finished tasks than `limit`. */
    @SerialName("has_more") val hasMore: Boolean = false,
    /** Queued and running, newest first. */
    val open: List<TaskSummary> = emptyList(),
)

@Serializable
data class TaskProgress(
    /** Asked to stop, and will at the next item. */
    val cancelling: Boolean = false,
    val done: Long,
    /** The kind's own step name; empty before the first update. */
    val phase: String,
    /** Zero while unknown. */
    val total: Long,
)

@Serializable
data class TaskSummary(
    val cancellable: Boolean = false,
    val error: String? = null,
    @SerialName("finished_at") val finishedAt: Long? = null,
    val id: Long,
    /** `scan`, `metadata_refresh` or `thumbnails`. */
    val kind: String,
    @SerialName("library_id") val libraryId: Long? = null,
    /** The library it works on, or its root's. */
    @SerialName("library_name") val libraryName: String? = null,
    /** While it has a place in the runner, including while it waits for its turn. */
    val progress: TaskProgress? = null,
    @SerialName("requested_at") val requestedAt: Long,
    @SerialName("root_id") val rootId: Long? = null,
    @SerialName("root_path") val rootPath: String? = null,
    @SerialName("started_at") val startedAt: Long? = null,
    /** `queued`, `running`, `ok`, `failed` or `cancelled`. */
    val state: String,
    /** What asked for it: `added`, `manual`, `scheduled` or `watch`. */
    val trigger: String,
)

@Serializable
data class TemplateCheck(
    /** What's wrong, when it isn't valid. */
    val error: String? = null,
    val pieces: List<TemplatePiece> = emptyList(),
    /** The normalised template, when valid. */
    val template: String? = null,
    val valid: Boolean = false,
)

@Serializable
data class TemplatePiece(
    val role: TemplateRole,
    val text: String,
)

/**
 * How a piece of a template is shown: grouping levels, the item folder, levels
 * below it, or a `/` separator.
 */
@Serializable
@JvmInline
value class TemplateRole(val value: String) {
    companion object {
        val Grouping = TemplateRole("grouping")
        val Item = TemplateRole("item")
        val Below = TemplateRole("below")
        val Separator = TemplateRole("separator")
    }
}

/** What came back from a test sign-in. Nothing was saved. */
@Serializable
data class TestReport(
    /** Every claim's name, from the ID token and userinfo. */
    val claims: List<String> = emptyList(),
    val email: String? = null,
    @SerialName("email_verified") val emailVerified: Boolean = false,
    /** As granted, if the provider said. */
    @SerialName("granted_scopes") val grantedScopes: String? = null,
    /** Missing when the groups claim wasn't there. */
    val groups: List<String>? = null,
    /** The name a new user would get. */
    val name: String? = null,
    val notes: List<Note> = emptyList(),
    /** What a real sign-in would have done with this account. */
    val outcome: String? = null,
    val provider: String,
    /** As asked for. */
    val scopes: String,
    val subject: String? = null,
)

@Serializable
data class TestResult(
    val error: String? = null,
    val ok: Boolean = false,
)

@Serializable
data class TestStarted(
    /** Open this in a new window; it ends by handing over the report's token. */
    val url: String,
)

@Serializable
data class ThumbnailsStarted(
    /** False when it has to wait: another of this library's is running or queued. */
    val started: Boolean = false,
)

@Serializable
@JvmInline
value class TransferMode(val value: String) {
    companion object {
        val Hardlink = TransferMode("hardlink")
        val Copy = TransferMode("copy")
        val Move = TransferMode("move")
    }
}

@Serializable
data class UpdateRoot(
    @SerialName("is_import_target") val isImportTarget: Boolean? = null,
    @SerialName("layout_template") val layoutTemplate: String? = null,
    @SerialName("write_policy") val writePolicy: String? = null,
)

@Serializable
data class User(
    @SerialName("created_at") val createdAt: Long,
    val email: String? = null,
    val id: Long,
    /** Hides what the user watches from other users, not from admins. */
    val incognito: Boolean = false,
    @SerialName("is_admin") val isAdmin: Boolean = false,
    /** A throwaway account from "Continue as guest" ([`crate::guests`]). */
    @SerialName("is_guest") val isGuest: Boolean = false,
    val name: String,
    val status: UserStatus,
)

@Serializable
data class UserRef(
    val id: Long,
    val name: String,
)

/** Whether a user may use the app. */
@Serializable
@JvmInline
value class UserStatus(val value: String) {
    companion object {
        val Active = UserStatus("active")
        val Pending = UserStatus("pending")
        val Disabled = UserStatus("disabled")
    }
}

@Serializable
data class UserUpdate(
    /** Empty to remove it. */
    val email: String? = null,
    @SerialName("is_admin") val isAdmin: Boolean? = null,
    val name: String? = null,
    /** Sets a new password, signing the user out everywhere. */
    val password: String? = null,
)

/** A file of the item, with what's in it. */
@Serializable
data class Version(
    val audio: List<AudioTrack> = emptyList(),
    /** `mp4`, `webm`, `mkv`, or ffprobe's own name for anything else. */
    val container: String? = null,
    @SerialName("duration_seconds") val durationSeconds: Double? = null,
    @SerialName("file_id") val fileId: Long,
    @SerialName("file_name") val fileName: String,
    @SerialName("hdr_format") val hdrFormat: String? = null,
    val height: Long? = null,
    val size: Long,
    val subtitles: List<SubtitleTrack> = emptyList(),
    @SerialName("video_codec") val videoCodec: String? = null,
    @SerialName("video_stream") val videoStream: Long? = null,
    val width: Long? = null,
)

@Serializable
data class View(
    /** `ordering:<id>` or `playlist:<id>`. */
    val id: String,
    @SerialName("is_default") val isDefault: Boolean = false,
    /** Ordering kind (`aired`, `dvd`, `absolute`, `story`, `custom`) or `playlist`. */
    val kind: String,
    val name: String,
    /** Playlists: visible to every user. */
    val shared: Boolean = false,
    /** Where it comes from: a provider (`tmdb`), `user`, or `smart`/`manual` for playlists. */
    val source: String,
)

@Serializable
data class WantedItem(
    @SerialName("airs_at") val airsAt: Long? = null,
    @SerialName("airs_time_known") val airsTimeKnown: Boolean = false,
    val availability: Availability,
    /** Height of the best present file (upgrades), and the cutoff it must reach. */
    @SerialName("best_height") val bestHeight: Long? = null,
    @SerialName("cutoff_height") val cutoffHeight: Long? = null,
    /** Provider IDs, so acquisition tools can look the item up. */
    @SerialName("external_ids") val externalIds: List<List<String>> = emptyList(),
    @SerialName("item_id") val itemId: Long,
    /** `episode` or `movie`. */
    val kind: String,
    /** Number in the series' default ordering. */
    val number: EpisodeNumber? = null,
    @SerialName("quality_profile") val qualityProfile: String? = null,
    /** `missing`, `not_acquired` or `upgrade`. */
    val reason: String,
    @SerialName("series_id") val seriesId: Long? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    /** Current external statuses (stale ones included, flagged). */
    val statuses: List<ExternalStatus> = emptyList(),
    val title: String,
)

@Serializable
data class WantedPage(
    val items: List<WantedItem> = emptyList(),
    val total: Long,
)

@Serializable
data class WatchState(
    @SerialName("last_played_at") val lastPlayedAt: Long? = null,
    val played: Boolean = false,
    @SerialName("position_seconds") val positionSeconds: Double,
)

@Serializable
data class Webhook(
    @SerialName("created_at") val createdAt: Long,
    /** The last event delivered. */
    val cursor: Long,
    val enabled: Boolean = false,
    /** Event types delivered; empty means all. */
    @SerialName("event_types") val eventTypes: List<String> = emptyList(),
    val failures: Long,
    val id: Long,
    @SerialName("last_delivered_at") val lastDeliveredAt: Long? = null,
    @SerialName("last_error") val lastError: String? = null,
    val name: String,
    @SerialName("next_attempt_at") val nextAttemptAt: Long,
    /** Events waiting to be delivered. */
    val pending: Long,
    val url: String,
)

@Serializable
data class WebhookInput(
    val enabled: Boolean? = null,
    @SerialName("event_types") val eventTypes: List<String>? = null,
    /** Start from the oldest kept event instead of now (create only). */
    @SerialName("from_start") val fromStart: Boolean? = null,
    val name: String? = null,
    val url: String? = null,
)

@Serializable
data class WebhookReceived(
    /** Roots rescanned because the payload named paths in them. */
    val rescanning: List<Long> = emptyList(),
)
