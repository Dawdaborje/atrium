# atrium_mailbox: design

A person's own mail, written in Rust. The mail stays on the organization's mail server (a JMAP server such as Stalwart, Cyrus or Fastmail, or any IMAP server such as Dovecot, with SMTP for sending); this
plugin keeps **no copy and no model**. Every page is a call to the kernel's `jmap` bridge.

## Whose mailbox

The login is the person's own and is never a plugin setting. On `/mailbox/connect` (the `<login bridge="jmap"/>` element) they give their user name
and password; the kernel checks them with the mail server, stores the password encrypted and never shows it again. When this plugin calls the
bridge, the kernel looks up the **caller's** login and hands it over: the plugin cannot read it, cannot name another person's, and one person can never
reach another's mailbox through it. Disconnecting forgets the login. An administrator switches the bridge on and sets
`bridge.jmap.session_url` (such as `https://mail.example.com/.well-known/jmap`).

The plugin tries the `jmap` bridge, and if the person has no login connected there (or the administrator has not set it up) the `imap` bridge. Both offer the same actions, so nothing else differs. On `/mailbox/connect` a person connects to the one their organization uses. A refusal that is not about a missing login (a wrong password, say) is shown as it is and does not move on to the next server.

## What it does

| Function | Does |
|---|---|
| `mailbox_state` | is the person connected, and as whom |
| `folders`, `inbox_messages` | folders with unread counts; the newest 50 messages of a folder (the inbox by default), searched by words, unread tagged "New" |
| `get_message` | one message: who, when, to, cc, attachment names, the text. Opening it marks it read. |
| `mark_unread`, `trash_message` | mark unread; move to the trash folder |
| `reply_message` | answer the sender (or everyone), with files if wanted, as a reply: `Re:` subject, `In-Reply-To` and `References` set, sent from the person's own address, kept in their Sent folder |
| `get_attachments` | the Attachments button of a message: fetches each file from the mail server and stores it with the kernel (`media::store`), readable by that person alone; shows view and download. Files the kernel refuses (unknown types) or larger than 10 MB are listed as not fetched |
| `send_message` | a new message to one or more addresses, with attached files (up to 10, 10 MB in all) |
| `can_read_media` | the plugin's media guard; always "no": the files are the person's own and are never shared |

## Safety

Mail text is shown as plain text, every line quoted, so nothing in a message is read as formatting or a link; a message with only HTML is shown
as its text with tags and scripts removed, and nothing remote is loaded. Addresses with control characters or spaces are refused (no header
injection). Sending uses the mail server's own submission, so the organization's SPF/DKIM, limits and sent history apply as for any mail client.

## Not built

Fetched attachments stay in the media store (guarded for their owner) until cleaned up; no drafts, folders other than the server's, or HTML composing; no push (the list is read when the page
opens).
