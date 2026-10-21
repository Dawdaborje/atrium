MODELS = [
    {"name": "site_site", "label": "Site", "plural": "Sites", "icon": "house", "title": "name", "fields": [
        {"name": "name", "type": "string", "required": True, "max_length": 120},
        {"name": "description", "type": "string", "max_length": 400},
        {"name": "home", "type": "text", "help": "The start page, in Markdown"},
        {"name": "owner", "type": "string", "required": True, "max_length": 128},
        {"name": "open", "type": "bool", "default": False, "help": "Everyone in the organization can visit it"},
        {"name": "show_news", "type": "bool", "default": False, "help": "Show the latest news on the start page"},
        {"name": "edited_at", "type": "datetime"},
    ]},
    {"name": "site_member", "label": "Site member", "plural": "Site members", "icon": "users", "title": "user", "fields": [
        {"name": "site", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
        {"name": "role", "type": "select", "default": "editor", "options": ["owner", "editor", "visitor"]},
    ]},
    {"name": "site_link", "label": "Site link", "plural": "Site links", "icon": "link", "title": "title", "fields": [
        {"name": "site", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "position", "type": "int", "default": 0},
        {"name": "title", "type": "string", "required": True, "max_length": 120},
        {"name": "url", "type": "string", "required": True, "max_length": 500},
        {"name": "about", "type": "string", "max_length": 200},
    ]},
]
