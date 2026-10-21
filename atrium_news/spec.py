CHATTER = {"enabled": True, "messages": True, "notes": False, "followers": True, "track_changes": False, "visitors": "none"}
MODELS = [
    {"name": "news_post", "label": "Post", "plural": "Posts", "icon": "newspaper", "title": "title",
     "reads": {"published_read": {"status": "published"}}, "chatter": CHATTER, "fields": [
        {"name": "title", "type": "string", "required": True, "max_length": 200},
        {"name": "summary", "type": "string", "max_length": 400},
        {"name": "body", "type": "text"},
        {"name": "category", "type": "string", "max_length": 60, "index": "plain"},
        {"name": "author", "type": "string", "required": True, "max_length": 128, "index": "plain"},
        {"name": "status", "type": "select", "default": "draft", "options": ["draft", "published"], "index": "plain"},
        {"name": "pinned", "type": "bool", "default": False},
        {"name": "published_at", "type": "datetime", "index": "plain"},
        {"name": "edited_at", "type": "datetime"},
    ]},
    {"name": "news_author", "label": "Author", "plural": "Authors", "icon": "pen", "title": "user", "fields": [
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
    {"name": "news_read", "label": "Read mark", "plural": "Read marks", "icon": "eye", "title": "user", "fields": [
        {"name": "post", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
    {"name": "news_like", "label": "Like", "plural": "Likes", "icon": "heart", "title": "user", "fields": [
        {"name": "post", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
]
