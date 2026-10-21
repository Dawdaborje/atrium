CHATTER = {"enabled": True, "messages": True, "notes": False, "followers": True, "track_changes": False, "visitors": "none"}
MODELS = [
    {"name": "mail_box", "label": "Box", "plural": "Boxes", "icon": "inbox", "title": "name", "fields": [
        {"name": "name", "type": "string", "required": True, "max_length": 120},
        {"name": "tag", "type": "string", "max_length": 40, "index": "plain", "help": "The part after + in the address; empty is the box for plain inbox@"},
    ]},
    {"name": "mail_member", "label": "Box member", "plural": "Box members", "icon": "users", "title": "user", "fields": [
        {"name": "box", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
    {"name": "mail_thread", "label": "Conversation", "plural": "Conversations", "icon": "mail",
     "title": "subject", "reads": {"members_read": {"box": "$my_boxes"}}, "chatter": CHATTER, "fields": [
        {"name": "box", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "subject", "type": "string", "max_length": 300},
        {"name": "from_name", "type": "string", "max_length": 200},
        {"name": "from_address", "type": "string", "max_length": 320, "index": "plain"},
        {"name": "status", "type": "select", "default": "open", "options": ["open", "pending", "closed"], "index": "plain"},
        {"name": "assignee", "type": "string", "max_length": 128, "index": "plain"},
        {"name": "last_at", "type": "string", "max_length": 40, "index": "plain"},
        {"name": "count", "type": "int", "default": 0},
    ]},
    {"name": "mail_message", "label": "Message", "plural": "Messages", "icon": "message-square", "title": "subject",
     "reads": {"members_read": {"box": "$my_boxes"}}, "fields": [
        {"name": "box", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "thread", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "direction", "type": "select", "default": "in", "options": ["in", "out"]},
        {"name": "subject", "type": "string", "max_length": 300},
        {"name": "sender", "type": "string", "max_length": 320},
        {"name": "by", "type": "string", "max_length": 128},
        {"name": "text", "type": "text"},
        {"name": "at", "type": "string", "max_length": 40},
        {"name": "message_id", "type": "string", "max_length": 400, "index": "plain"},
    ]},
]
