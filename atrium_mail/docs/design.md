# atrium_mail: design

Shared inboxes, written in Rust. Mail to `inbox@<mail domain>` becomes a conversation that a team answers together.
Built on the kernel's [inbound mail](../../../../aether/docs/architecture/inbound_mail.md); the plugin holds no mail server.

## How mail gets here

`plugin.toml` declares one route, `[[mail_routes]] alias = "inbox"` with the function `on_mail`. The administrator makes the
intake's secret (`POST /api/ui/mail/secret`), sets `mail.inbound.domain`, and points the mail provider at
`/api/mail/<organization>/<provider>`. The kernel checks the secret, drops mail no person wrote (bounces, bulk), remembers
message ids against retries, and calls `on_mail` with a clean message.

## Models

| Model | Holds |
|---|---|
| `mail_box` | `name`, `tag`: the part after `+`. Tag `sales` is `inbox+sales@...`; an empty tag is plain `inbox@...` |
| `mail_member` | `box`, `user`: who works the box |
| `mail_thread` | `box`, `subject` (without `Re:`), `from_name`, `from_address`, `status` (`open`, `pending`, `closed`), `assignee`, `last_at`, `count`; chatter on |
| `mail_message` | `thread`, `box`, `direction` (`in`, `out`), `sender`, `by`, `text`, `at`, `message_id` |

Reads of threads and messages are limited to the people of the box (`$my_boxes`, answered by `rule_var_my_boxes`), which is what keeps the
chatter of a conversation from strangers. Writes go through the plugin's functions.

## Which box, which conversation

`on_mail` looks at every recipient: `inbox+tag@` finds the box with that tag (case does not matter), `inbox@` the box without
a tag. An unknown tag falls back to the box without a tag; with no such box the mail is not kept. A message is placed in a conversation by
(1) the `[#key]` that replies carry in the subject, (2) `In-Reply-To` naming a message already kept, else (3) it opens a new conversation.
A message id that was kept already is ignored. Any new message sets the conversation `open` again. The assignee is told; with no assignee,
everyone in the box is.

## Working a conversation

Anyone in the box can take, let go, assign (to a person of the box), close and reopen, and read. `reply_thread` sends an email to the
sender (`communication::send`) with the subject `Re: <subject> [#key]` and a `Reply-To` of the box's own address (the domain comes from `mail::thread`),
so the customer's answer comes back to `on_mail` and joins the same conversation. A sent reply sets the state to `pending` and gives
the conversation to the sender if nobody had it. If the mail cannot be sent (no mail bridge in the settings) the reply is refused with the reason and nothing is kept.
Internal talk goes in the chatter of the conversation, which only the box's people can open.

Administrators make and delete boxes and choose their people (`/mail/boxes`). A box with conversations is not deleted.

## Limits and gaps

- Text only: HTML mail arrives as its text part, attachments are not shown yet (the kernel stores them; the plugin does not list them).
- Reply addresses of the kernel (`inbox+tk_...`, which put answers into the chatter) are deliberately not used: answers belong in the conversation.
- No personal mailboxes and no sending as the person: that is `atrium_mailbox`, over the JMAP or IMAP bridge.
- `mail::unmatched` and `mail::reply` are not built in the kernel; mail nobody has a box for stays in the administrator's queue.
