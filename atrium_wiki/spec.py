HIST = {"enabled": True, "keep_days": 2555, "fields": "all"}
CHATTER = {"enabled": True, "messages": True, "notes": False, "followers": True, "track_changes": True, "visitors": "none"}
MODELS = [
    {"name": "wiki_space", "label": "Space", "plural": "Spaces", "icon": "book-open", "title": "name", "fields": [
        {"name": "name", "type": "string", "required": True, "max_length": 120},
        {"name": "description", "type": "text"},
        {"name": "owner", "type": "string", "required": True, "max_length": 128},
        {"name": "open", "type": "bool", "default": False, "help": "Everyone in the organization can read it; only its people write"},
    ]},
    {"name": "wiki_member", "label": "Space member", "plural": "Space members", "icon": "users", "title": "user", "fields": [
        {"name": "space", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
        {"name": "role", "type": "select", "default": "editor", "options": ["owner", "editor", "reader"]},
    ]},
    {"name": "wiki_page", "label": "Page", "plural": "Pages", "icon": "file-text", "title": "title",
     "reads": {"readers_read": {"space": "$my_spaces"}}, "history": HIST, "chatter": CHATTER, "fields": [
        {"name": "space", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "parent", "type": "string", "max_length": 64, "index": "plain", "help": "The key of the page this one is under; empty at the top"},
        {"name": "title", "type": "string", "required": True, "max_length": 200, "track": True},
        {"name": "body", "type": "text", "track": True, "help": "Markdown; [[Another page]] links to a page of the same space"},
        {"name": "updated_by", "type": "string", "max_length": 128},
        {"name": "edited_at", "type": "datetime"},
    ]},
]
