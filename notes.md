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
