# Atrium (home)

The entry to the Atrium workspace. It has no data of its own: one page, `/atrium`, with a heading carrying the logo and
tiles that lead to the modules.

## Why a plugin of its own

The Apps launcher makes the member of a workspace with the shortest plugin name the *host* of the workspace folder, and
folds the members that have a single page into the host's sidebar. Before this plugin the host was whichever `atrium_*`
plugin sorted first, so opening Atrium landed in Chat. Named `atrium`, this plugin is always the host: opening Atrium
opens the home page, and the single-page modules (chat, news, forms, …) are reached from its sidebar and from the tiles.
Modules with several pages (Drive, Planner, …) keep their own tile in the folder.

## The tiles

`<tiles>` and `<tile>` are page widgets of the platform (see `aether/docs/plugin/pages.md`). A tile shows the logo of
the plugin named in `plugin` when it ships one, else `icon`. The links are fixed to the routes of the Atrium modules; a
module that is not installed in an organization leads to a missing page, which does not happen when the workspace is
installed as a whole.
