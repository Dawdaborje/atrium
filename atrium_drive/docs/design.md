# atrium_drive: design

Folders and files for Atrium, written in Rust. The bytes live in the kernel's media store; the drive keeps the folders, the
names, who may see what, and every version of a file.

## Models

| Model | Holds |
|---|---|
| `drive_folder` | `name`, `parent` (a folder key; empty at the top), `owner`, `trashed`, `modified_at` |
| `drive_file` | `name`, `folder`, `owner`, `media` (the current version: key, name, mime, size), `mime`, `size`, `version`, `trashed`, `modified_at`, `modified_by` |
| `drive_blob` | One row for each stored version of a file: `file`, `blob_key`, `version`, `media`, `uploaded_by`, `at`. The `media` field makes the kernel's cleanup keep every version, not only the current one |
| `drive_share` | `item`, `kind`, `user` (a person's id, or `org` for everyone), `level` (`view` or `edit`), `granted_by` |
| `drive_link` | `item`, `kind`, `created_by`, `expires_at` (the last day it works; empty: never), `revoked`, `at`. The link's token is the key of its row |
| `drive_comment` | `file`, `author`, `body`, `at` |
| `drive_favourite` | `user`, `item`, `kind` |
| `drive_setting` | One row for the organization: `quota_mb`, `links_enabled`, `link_days`, `link_days_max`, `search_contents`, `comments_enabled` |

`drive_file` also keeps `content` (the text found in the file, at most 60,000 characters) and `content_version` (the version it came from).

Rules allow only the plugin's own functions to touch these (`via:self`); who may see what is decided in code.

## Access

A person's level on an item is the best of: **owner** (the item's owner, or the owner of any folder above it: what is in
your folder is yours to manage), or the highest **share** on the item or any folder above it. A share is `view` or `edit`;
ownership is never granted by a share. An item the person has no level on does not exist for them (`file_info`,
`drive_browse` and the rest answer "there is no such file"). Sharing and deleting for good are for the owner; renaming,
moving, trashing and adding versions need `edit`; adding to a folder needs `edit` on it.

## The files themselves

The plugin declares `[media_guard] function = "can_read_media"` and calls `media::protect` for every key it registers (an
upload through `/api/media` first, then `register_file` or `new_version` at once). The kernel then serves a key (download,
extracted text, thumbnail) only when `can_read_media` allows the person asking: some file with that key, at least `view`,
and not in the trash unless they own it. Every version keeps its key, so a person who can read the file can read each
version. After a file is deleted for good its versions are gone from the drive, so nobody can read the stored bytes, even
before the kernel's cleanup removes them.

## Versions, trash, names

A new version adds a `drive_blob` row and makes it current; restoring an old version adds another row pointing at the same
stored file, so history only grows and nothing is lost either way. Trashing sets a flag on the item (a folder's contents
disappear with it); the owner can restore, or delete for good (a folder takes everything in it). A name is unique inside a
folder, ignoring case: an upload with a taken name becomes `name (2).ext`; creating or renaming to a taken name is refused.

## Functions

Looking: `drive_browse` (`{ folder }` or `{ view }` = `mine`, `shared`, `recent`, `trash`), `search_files`, `file_info`,
`file_versions`, `list_shares`, `folder_tree`, `storage_used`, `drive_nav`, `list_people`. Changing: `register_file`,
`new_version`, `restore_version`, `create_folder`, `rename_item`, `move_item`, `trash_item`, `restore_item`, `purge_item`,
`empty_trash`, `share_item`, `unshare_item`. Guard: `can_read_media`. Registering a file emits the event `file_added`; a share
notifies the person it is shared with.

## Guarded from the first moment

The page uploads with `guard="true"`, so the kernel guards the file as it is stored: until `register_file` or `new_version`
calls `media::protect`, only the uploader can read it. There is no moment when other members can fetch an unregistered file.

## Search

`search_files` finds files and folders by name and files by the text inside them (`found_in` says which). When a file is
registered, gets a new version or has a version restored, the drive asks the kernel to read it (`ocr::submit`, a PDF, an
Office file, a scan or a plain-text file) and `index_done` keeps the text on the row. A kind of file the kernel cannot
read is found by its name only; files stored before the setting was switched on are not read again.

## Links

`create_link` (owner only) makes a link to a file or folder: anyone in the organization who has it can open it for viewing
at `/drive/link/{token}`, and for a folder walk through what is inside. It works until its last day, or until the owner takes
it back (`revoke_link`), or until the item is trashed or deleted. The kernel serves the stored bytes of a file a working link
covers to any member who asks with the key; the key is only given out by the link or by the drive. Links are for the
organization's members; a link for people outside it waits for the kernel's sharing proposal.

## Comments and favourites

Anyone who can view a file can comment on it (`file_comments`, `add_comment`); the author and the file's owner can delete a
comment, and the owner is told of a new one. A person stars files and folders for themselves (`toggle_favourite`); the
Favourites place lists what they can still see.

## What an organization sets

An administrator (`org_admin`) opens `/drive/settings` (a link in the drive's header) to set: the **storage** each person may
use in MB (every stored object is counted once, however many versions use it; 0 is no limit), whether **links** are allowed,
how many days a new link works and the longest one may, whether search reads the **text inside files**, and whether
**comments** are on. A plugin role of its own would have shut the other drive pages to people without it, so the
settings are for the organization's administrators.

## Functions

Looking: `drive_browse` (`{ folder }` or `{ view }` = `mine`, `favourites`, `shared`, `recent`, `trash`), `search_files`,
`file_info`, `file_versions`, `list_shares`, `list_links`, `file_comments`, `open_link`, `folder_tree`, `storage_used`
(`bytes` and `quota_bytes`), `drive_nav`, `list_people`, `drive_settings`. Changing: `register_file`, `new_version`,
`restore_version`, `create_folder`, `rename_item`, `move_item`, `trash_item`, `restore_item`, `purge_item`, `empty_trash`,
`share_item`, `unshare_item`, `create_link`, `revoke_link`, `add_comment`, `delete_comment`, `toggle_favourite`,
`save_drive_settings`. Guard: `can_read_media`. Call back: `index_done`. Registering a file emits `file_added`; a share and
a comment notify the person concerned.

## Editing office files online

When the administrator has set up an editor server (the kernel's `collabora` bridge), word-processor, spreadsheet and presentation files (doc, docx,
xls, xlsx, ppt, pptx, odt, ods, odp, rtf, csv) offer "Edit online" in the row menu (people who may only read get "Open in editor", without saving).
`open_in_editor` checks the person's level, then asks the bridge for an address made for this file and this person; the editor opens in a new tab.
The editor reads the file from the kernel and sends every save back through it; the kernel calls `new_version` **as the person**, so a save is
refused if they may no longer edit, counts against the quota, and is a version that can be restored. Two people in one editor session co-edit in the
editor; the drive sees each save. Autosaves make versions: restore or purge old ones as for any version. Not built: opening inside the viewer's frame.

## Limits and what is not built

Uploads are what the media store accepts (25 MB; pictures, PDF, Office files, zip and plain text). Not built: links for people
outside the organization (they wait for the kernel's sharing proposal), a sync client (the record API and API tokens are
there for one to be written), and per-person quotas (the quota is one number for everyone).
