# atrium_sites: design

A site is a start page for a team, a department or the whole organization. It composes the other Atrium modules
instead of copying them: the words are Markdown, the news comes from `atrium_news`, and the links lead to a wiki space, a drive
folder, a plan, a form or any web address.

## Models

| Model | Holds |
|---|---|
| `site_site` | `name`, `description`, `home` (Markdown), `owner`, `open`, `show_news`, `edited_at` |
| `site_member` | `site`, `user`, `role` (`owner`, `editor`, `visitor`) |
| `site_link` | `site`, `position`, `title`, `url`, `about` |

All three are closed by their rules (`via:self`); people reach them through the plugin's functions.

## Access

A site is invisible to people who are not in it unless it is open to the organization, in which case everyone visits it and
only its people change it. Visitors look, editors write the start page, show or hide the news and manage the links, owners also
rename the site, open or close it, delete it and manage its people. A site keeps one owner at least.

## The start page

`view_site` renders Markdown: the one-line description, the home text, "Latest news" (the five newest published posts, when
`show_news` is on and `atrium_news` is installed; it is an optional dependency, so the site works without it) and "Quick links".
A link is a path of this app (`/wiki/s/abc`) or an `https://` address; anything else (`//host`, `javascript:`, spaces) is
refused. Links reorder with Move up and Move down. The news shown is what the visitor may read, because the news plugin is
called as them.

## Known limits

No several pages in a site (use a wiki space and link to it), no layout or web parts, no site templates, no public sites for
people outside the organization (that waits for the sharing proposal), no analytics, no follow-a-department link: the HR link
could create sites for departments later.

## Tests

`tests/sites_e2e.py` (HTTP). Unit tests (`cargo test`) cover link addresses and the rendered page.
