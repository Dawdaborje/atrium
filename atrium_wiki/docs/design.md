# atrium_wiki: design

Spaces of Markdown pages, written in Rust. A page's conversation (comments, followers, the log of what changed) is the
kernel's chatter, and its past is the kernel's history.

## Models

| Model | Holds |
|---|---|
| `wiki_space` | `name`, `description`, `owner`, `open` |
| `wiki_member` | `space`, `user`, `role` (`owner`, `editor` or `reader`) |
| `wiki_page` | `space`, `parent` (the key of the page above, empty at the top), `title`, `body`, `updated_by`, `edited_at`; history and chatter on |

Rules let only the plugin's own functions touch these (`via:self`). `wiki_page` may also be read by anyone whose space list
holds the page's space (`"space": "$my_spaces"`, answered by `rule_var_my_spaces`: their spaces and the open ones), which is
what shows a page's conversation to the people who can read the page.

## Access

A space is invisible to people who are not in it, unless it is open: then everyone in the organization reads it. Readers
read, editors make, change, move and delete pages, owners also rename the space, open or close it, delete it and manage its
people. A space keeps one owner at least; anyone may leave.

## Pages

A title is unique within a space (ignoring case) and cannot hold `[`, `]` or `|`. A page goes under another page of the same
space, never under itself or one of its own subpages. Deleting a page lifts its subpages one level; the page's history stays in
the kernel. The text is Markdown (200,000 characters at most); `[[Title]]` and `[[Title|words]]` become links to the page
of that title in the same space, and a title with no page stays as typed. A page shows who links to it, and its subpages.

## History

`history: enabled` on the model. `page_history` lists the versions and `restore_version` puts the page back to a version as a
new version, so nothing is lost. The kernel's Undo button on the edit form works as well.

## Search

`search_pages` looks at titles and text of the pages in the spaces the person reads: exact title first, then title contains,
then text, with a snippet. Plain text search, no ranking by meaning (the kernel's `ai::search` can be added when a model is
configured). `recent_pages` lists the 30 latest edits.

## Pages of the plugin

`/wiki`, `/wiki/new`, `/wiki/search`, `/wiki/recent`, `/wiki/s/{space}` (tree, People, Settings), `/wiki/s/{space}/new`,
`/wiki/p/{id}` (the page rendered, Edit, History, comments), `/wiki/p/{id}/edit`, `/wiki/p/{id}/history`.

## Known limits

Markdown only: there is no rich-text editor yet (that needs a kernel field type), no pictures inside a page (link a file from
the drive), no page templates, no moving a page between spaces, no import. A page is edited by one person at a time; the
last save wins (history keeps the other).

## Tests

`tests/wiki_e2e.py` (HTTP). Unit tests (`cargo test`) cover links and snippets.
