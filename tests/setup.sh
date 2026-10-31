#!/usr/bin/env bash
# Makes the throwaway instance the end-to-end scripts here talk to: a configuration, the core database, an organization
# "atr_org" with an administrator (owner) and two ordinary members (bob, cy), the base and HR plugins and the Atrium
# plugins. Then start the
# server: ./serve.sh
#
#   DB_PORT=8000 ./setup.sh        # SurrealDB must be running there (user root, password root)
#
# It is meant for an empty database. The aether binary is the one built in ../../../aether (AETHER_BIN overrides).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${AETHER_BIN:-$here/../../../aether/target/debug/aether}"
port="${DB_PORT:-8000}"
cat > "$here/aether.toml" <<TOML
app_dir = "$here/app_dir"
[core]
instance_name = "Atrium e2e"
[server]
host = "127.0.0.1"
port = 17892
[configuration]
is_development_mode = true
is_development_with_assets = false
[database]
host = "127.0.0.1"
port = $port
user = "root"
password = "root"
namespace = "atr_e2e"
[cache]
backend = "moka"
default_ttl_secs = 300
max_entries = 10000
max_value_bytes = 1048576
[media]
backend = "local"
# People outside the organization (public booking pages) say which organization they mean with a header.
[tenancy]
org_resolution = "header"
# The built-in video meeting server (atrium_meet), on a port of its own.
[sfu]
enabled = true
bind = "127.0.0.1:17893"
public_ips = ["127.0.0.1"]
TOML
cd "$here"
a() { "$bin" -c "$here/aether.toml" "$@"; }
a --init --admin-username admin --admin-email admin@test.local --admin-password Test-pass-123 >/dev/null
a --create-org "Atr Org" --org-db-name atr_org --username owner --email owner@test.local --password Test-pass-123 >/dev/null
# The base plugins and HR (atrium_hr_link reads them), then Atrium.
a --load-plugins "$here/../../base" >/dev/null
for plugin in base_ui currency country party company calendar number_sequence; do
  a --install-plugin "$plugin" --org atr_org >/dev/null
done
a --load-plugins "$here/../../hrms/hr" >/dev/null
a --install-plugin hr --org atr_org >/dev/null
# Leave (atrium_hr_link marks people on approved leave as away); it needs the compensation plugin.
for plugin in hr_compensation hr_leave; do
  a --load-plugins "$here/../../hrms/$plugin" >/dev/null
  a --install-plugin "$plugin" --org atr_org >/dev/null
done
for plugin in atrium_chat atrium_drive atrium_planner atrium_people atrium_hr_link atrium_calendar atrium_booking atrium_wiki atrium_news atrium_forms atrium_sites atrium_mail atrium_meet atrium_mailbox; do
  (cd "$here/../$plugin" && make >/dev/null)
  if [ -d "$here/../$plugin/models" ]; then a --sync-models "$here/../$plugin" >/dev/null; fi
  a --load-plugins "$here/../$plugin" >/dev/null
  a --install-plugin "$plugin" --org atr_org >/dev/null
done
for user in bob cy; do
  a --create-user --username "$user" --email "$user@test.local" --password Test-pass-123 --org-db-name atr_org >/dev/null
  # A page needs a role when the organization has none of the plugin's own; this one is harmless.
  a --grant-role user_manager --username "$user" --org atr_org >/dev/null
done
a --seed >/dev/null
# The bridge switches (jitsi, collabora, jmap, smtp) need a developer account
a --grant-developer --username owner >/dev/null

echo "Ready. Start the server with ./serve.sh, then run the *_e2e.py scripts."
