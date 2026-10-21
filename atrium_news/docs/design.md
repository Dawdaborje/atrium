# atrium_news: design

News for the whole organization, written in Rust. The conversation under a post (comments, followers) is the kernel's chatter.

## Models

| Model | Holds |
|---|---|
| `news_post` | `title`, `summary`, `body` (Markdown), `category`, `author`, `status` (`draft` or `published`), `pinned`, `published_at`, `edited_at`; chatter on |
| `news_author` | `user`: a person who may write besides the administrators |
| `news_read` | `post`, `user`: who has opened a published post |
| `news_like` | `post`, `user` |

Rules let only the plugin's own functions write. A published post may also be read by everyone (`"status": "published"`),
which is what lets the kernel show its comments to the organization; a draft is read only by its author and administrators,
through the plugin.

## Who can do what

Organization administrators (`org_admin`) and the people they add on `/news/authors` can write, publish, edit and delete
their own posts; administrators can change and delete any post. Everyone reads the published ones, likes them and
comments. A draft does not exist for other people ("there is no such post").

## Publishing

`create_post` saves a draft (or publishes at once with `publish`), `publish_post` publishes and sends a notification to every
member except the author (up to 1,000; `notify: false` keeps it quiet), `unpublish_post` takes a post back to a draft. Pinned
posts come first in the feed, then the newest. The feed is searched by title, one-liner and text, and filtered by category.
A post that someone has not opened yet is tagged "New". The author sees how many people read a post, and who.

## Known limits

Markdown only, no pictures inside a post (link a file from the drive), no scheduled publishing, no audiences (the whole
organization reads everything), no expiry. Likes are one kind. Posts do not go to a site's start page yet (see `atrium_sites`).

## Tests

`tests/news_e2e.py` (HTTP).
