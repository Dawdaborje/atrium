KINDS = ["text", "long", "choice", "multi", "dropdown", "rating", "yes_no", "number", "date"]
MODELS = [
    {"name": "frm_form", "label": "Form", "plural": "Forms", "icon": "clipboard-list", "title": "title", "fields": [
        {"name": "owner", "type": "string", "required": True, "max_length": 128, "index": "plain"},
        {"name": "title", "type": "string", "required": True, "max_length": 200},
        {"name": "description", "type": "text"},
        {"name": "status", "type": "select", "default": "draft", "options": ["draft", "open", "closed"], "index": "plain"},
        {"name": "anonymous", "type": "bool", "default": False, "help": "Answers are kept without the name of who gave them"},
        {"name": "once", "type": "bool", "default": True, "help": "Each person answers once"},
        {"name": "closes_at", "type": "datetime"},
        {"name": "thanks", "type": "string", "max_length": 300},
        {"name": "edited_at", "type": "datetime"},
    ]},
    {"name": "frm_question", "label": "Question", "plural": "Questions", "icon": "circle-help", "title": "label", "fields": [
        {"name": "form", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "position", "type": "int", "default": 0},
        {"name": "label", "type": "string", "required": True, "max_length": 300},
        {"name": "kind", "type": "select", "default": "text", "options": KINDS},
        {"name": "options", "type": "json", "help": "The choices, for choice, multi and dropdown questions"},
        {"name": "required", "type": "bool", "default": False},
        {"name": "help", "type": "string", "max_length": 300},
    ]},
    {"name": "frm_response", "label": "Response", "plural": "Responses", "icon": "check-square", "title": "form", "fields": [
        {"name": "form", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "max_length": 128, "index": "plain", "help": "Empty when the form is anonymous"},
        {"name": "answers", "type": "json", "help": "{ question key: answer }"},
        {"name": "submitted_at", "type": "datetime", "index": "plain"},
    ]},
    {"name": "frm_done", "label": "Answered", "plural": "Answered", "icon": "check", "title": "user", "fields": [
        {"name": "form", "type": "string", "required": True, "max_length": 64, "index": "plain"},
        {"name": "user", "type": "string", "required": True, "max_length": 128, "index": "plain"},
    ]},
]
