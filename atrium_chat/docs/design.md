# atrium_chat: design

Team chat for Atrium, written in Rust. It is the first consumer of the kernel's [realtime](../../../../aether/docs/architecture/realtime.md)
WebSocket and of the chat widgets (`<panes>`, `<rail>`, `<messages>`, `<composer>`, `<presence>`).

## Models

| Model | Holds |
|---|---|
| `chat_team` | A named group of channels (Ops, Sales). Only an administrator makes teams. |
| `chat_channel` | A conversation: `kind` is `channel`, `group` or `direct`. `is_private`, `archived`, `topic`, `last_n` (the number of the last message, the source of sequence and unread counts) and, for a direct chat, `direct_key` (the two people, sorted, so a pair always finds one chat). |
| `chat_member` | A person in a conversation: `role` (`owner` or `member`), `last_read_n`, `muted`. |
| `chat_message` | `n` (position in its channel), `at` (when), `author`, `author_name` (kept at posting time), `body`, `files`, `parent` (the message it replies to), `reply_count`, `reactions` (`{ emoji: [users] }`), `mentions` and `mention_names`, `edited_at`, `deleted`. |
| `chat_mention` | One row for each unread mention of a person: `channel`, `user`, `message`, `n`. Reading the channel clears them. |

Rules allow only the plugin's own functions to touch these (`via:self`): who may see what is decided in code, not by
record rules, because "is a member of this channel" is a question about another record.

## Who may do what

- A public channel (`kind = channel`, not private, not archived) can be **read by anyone** in the organization without
  joining; posting joins the poster. Private channels, groups and direct messages are for their members only.
- A channel is managed (renamed, archived, people added or removed) by its owner or an administrator. In a group chat
  anyone in it may add people. A direct message cannot be left, added to or removed from.
- Anyone may edit their own message; the author, the channel's owner or an administrator may delete one.
- Messages are text of at most 8000 characters, with up to 10 files that must come from the media store.

## Live topics

| Topic | Joined by | Carries |
|---|---|---|
| `channel.{channel}` | whoever may read the channel | `message` `{ row }` (new, edited, or a parent whose reply count changed), `deleted` `{ id }`; client event `typing`; presence |
| `thread.{thread}` | whoever may read the channel of the message | the replies, `deleted`, `typing` |
| `inbox.{user}` | only that person | `activity` `{ channel, n }`: the rail asks `my_channels` again |
| `online` | everyone | presence only: being on the chat page is being online |

The kernel asks `can_join_*` before letting anyone in, as the person who asks; removing someone from a channel also
ends their live membership (`realtime::drop`).

## Functions

Starting: `my_channels` (the rail, with unread and mention counts), `browse_channels`, `create_channel`, `create_group`, `open_direct`, `create_team`,
`list_teams`, `list_people`, `people`. Belonging: `join_channel`, `leave_channel`, `add_members`, `remove_member`,
`edit_channel`, `channel_info`, `mark_read`. Messages: `post_message`, `edit_message`, `delete_message`,
`channel_messages` (oldest first, `{ limit, before }`), `thread_messages`, `react` (toggles the caller's reaction), `search_messages` (the conversations the caller can read, newest first), `channel_members`, `channel_header`, `set_muted`, `my_membership`, `pin_message` (toggles the pin; members and administrators) and `pinned_messages`. The heading has **Pinned** (the pinned page, with Unpin) and **Alerts** (mute for the caller). Posting emits the event `message_posted`
(`channel`, `message`, `author`, `reply`) for other plugins.

## Mentions

`@Name` in a body (a display name, ending at a word boundary) mentions that person; `@channel` and `@everyone` mention
everyone in the channel. Only people who can read the conversation are mentioned. A mentioned person gets a `chat_mention`
row (the rail shows a red `@N`), an inbox signal even when they muted the channel, and a notification with a link.

## For other plugins

`ensure_channel`, `sync_members` and `remove_everywhere` set up channels for plugins that keep them in step with something
of their own (a department, a project). They accept administrators and the kernel's own work (an event handler, a job);
see `atrium_hr_link`.

## Pages

`/chat/{channel?}` is the app: a rail of conversations (with unread badges and online dots), the channel with who is
here, its messages and composer, and a thread pane that opens with `?thread=<id>`. `/chat/new_channel`,
`/chat/new_message` and `/chat/new_group` are forms; `/chat/browse` lists open channels; `/chat/search` searches messages;
`/chat/{channel}/members` and `/chat/{channel}/settings` manage a conversation (add and remove people, rename, archive, leave).

## Not built yet

Message menus beyond pin, edit, delete and react, full-text
search with ranking, notifications settings, and typing in direct messages across tabs.
