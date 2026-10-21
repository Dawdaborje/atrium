MODELS = [
    {"name": "meet_meeting", "label": "Meeting", "plural": "Meetings", "icon": "video", "title": "title", "fields": [
        {"name": "title", "type": "string", "required": True, "max_length": 200},
        {"name": "host", "type": "string", "required": True, "max_length": 128, "index": "plain"},
        {"name": "room", "type": "string", "required": True, "max_length": 80, "index": "plain"},
        {"name": "starts_at", "type": "string", "max_length": 40, "index": "plain"},
        {"name": "minutes", "type": "int", "default": 30},
        {"name": "about", "type": "text"},
    ]},
    {"name": "meet_guest", "label": "Guest", "plural": "Guests", "icon": "user", "title": "user", "fields": [
        {"name": "meeting", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
]
