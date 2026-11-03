# Task system, with resumable task and tracked task progress

Example tasks:

- Generate thumbnails
- Trickplay generation
- AI Subtitle generation (in the future)
- AI Subtitle sync (in the future)
- Library scan
- Metadata fetch
- ....

Architectual thoughts, should we do subtasks? should a library wide generate thumbnail task:
a) not use any sub tasks, loop over everything and track progress, generate thumbnails one by one
b) loop over everything, generate a thumbnail task for every item (unless there's already one), and let the task system schedule this?
c) loop over everything, summing a sub task one at a time

For each task if there's logs, they should be scoped to the task and we should be able to see things logged in the task in the task inspector

# Activity log

Logging actions, events, library additions, user actions and so on

Some activity logs are confidential, meaning only admins can see them.

For example:

- Library item additions, new shows, new episodes, etc...
- Configuration changes (confidential)
- User started/stopped playing something, user watched something for x seconds, to completion etc... (maybe filter out short play sessions as to not be too spammy)
  - Users can decide to enable incognito mode, and in that mode only admins would be able to see their watch activity
- Logins, new users, etc (confidential?)
- Social stuff planned (comments, ratings, reviews, and so on)

# Subtitle management

A subtitle sync feature.

using

a) audio waveforms with some algorithm to exclude music somehow?
b) AI speech to text (think - whisper)
c) visual editor with waveforms and timeline

# Player is responsible for making decision on formats

I'm not sure how it works currently, but a thing that pops up often in other things are playback issues because of quirks in specific hardware, even it it reports it as supported.

Very often they just show an error and nothing else

Ideally, the player would ask for whatever streams it want, if it fails, it would ask for a transcoded stream, I.E the player makes the decisions, how is it done elsewhere?

# Native Oauth (openid connect)

....

# Native forward auth support

....

# Improved classification of subtitles and audio tracks

On subtitles i imagine we can use a pretty standard library to detect language, but then its the type of subtitle as well, commentary, SDH etc...

for audio, it's a bit worse

# Library permission model
