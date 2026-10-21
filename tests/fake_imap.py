"""A small IMAP server and a small SMTP server for the end-to-end scripts: enough of RFC 3501 for the kernel's `imap` bridge, and of
RFC 5321 for what it sends. Two accounts (ann / pw-ann, dan / pw-dan), each with INBOX, Drafts, Sent and Trash. Not a mail server."""
import base64, re, socketserver, threading
from email import message_from_bytes, policy
from email.message import EmailMessage

USERS = {"ann": "pw-ann", "dan": "pw-dan"}
BOXES = ("INBOX", "Drafts", "Sent", "Trash")
ATTRS = {"INBOX": "", "Drafts": "\\Drafts", "Sent": "\\Sent", "Trash": "\\Trash"}


class Store:
    def __init__(self):
        self.lock = threading.Lock()
        self.uid = 0
        self.boxes = {u: {b: [] for b in BOXES} for u in USERS}
        self.submitted = []  # {user, from, to, raw, message}
        self.logins = 0

    def add(self, user, subject, text, sender="Zed <zed@client.test>", seen=False, box="INBOX", date="19-Oct-2026 10:00:00 +0000", html=None, files=(), raw=None):
        if raw is None:
            m = EmailMessage()
            m["From"], m["To"], m["Subject"], m["Message-ID"] = sender, f"{user}@acme.test", subject, f"<m{self.uid + 1}@client.test>"
            if html:
                m.set_content("x")
                m.clear_content()
                m.add_alternative(html, subtype="html")
            else:
                m.set_content(text)
            for name, kind, data in files:
                main, _, sub = kind.partition("/")
                m.add_attachment(data, maintype=main, subtype=sub, filename=name)
            raw = m.as_bytes()
        self.uid += 1
        self.boxes[user][box].append({"uid": self.uid, "raw": raw, "flags": {"\\Seen"} if seen else set(), "date": date})
        return self.uid


STORE = Store()


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def parse_args(line):
    """Words of a command line, with quoted strings and (lists) kept whole."""
    out, i = [], 0
    while i < len(line):
        c = line[i]
        if c == " ":
            i += 1
        elif c == '"':
            j, buf = i + 1, ""
            while line[j] != '"':
                if line[j] == "\\":
                    j += 1
                buf += line[j]
                j += 1
            out.append(buf)
            i = j + 1
        elif c == "(":
            depth, j = 0, i
            while True:
                depth += (line[j] == "(") - (line[j] == ")")
                j += 1
                if depth == 0:
                    break
            out.append(line[i:j])
            i = j
        else:
            j = i
            while j < len(line) and line[j] != " ":
                j += 1
            out.append(line[i:j])
            i = j
    return out


def searchable(raw):
    """The decoded subject and text of a message, lower case: what a real server's TEXT search looks in."""
    m = message_from_bytes(raw, policy=policy.default)
    body = m.get_body(preferencelist=("plain", "html"))
    return ((m["Subject"] or "") + " " + (body.get_content() if body else "")).lower()


class Handler(socketserver.StreamRequestHandler):
    def send(self, text):
        self.wfile.write(text if isinstance(text, bytes) else text.encode())
        self.wfile.flush()

    def read_command(self):
        line = self.rfile.readline()
        if not line:
            return None
        line = line.rstrip(b"\r\n").decode("utf-8", "replace")
        literals = []
        while True:
            m = re.search(r"\{(\d+)\+?\}$", line)
            if not m:
                return line, literals
            n = int(m.group(1))
            self.send("+ go ahead\r\n")
            data = self.rfile.read(n)
            literals.append(data)
            line = line[: m.start()] + "\x00LIT%d\x00" % (len(literals) - 1) + self.rfile.readline().rstrip(b"\r\n").decode("utf-8", "replace")

    def handle(self):
        self.user, self.box = None, None
        self.send("* OK fake imap ready\r\n")
        while True:
            got = self.read_command()
            if got is None:
                return
            line, lits = got
            tag, _, rest = line.partition(" ")
            cmd, _, args = rest.partition(" ")
            cmd = cmd.upper()
            try:
                with STORE.lock:
                    done = self.dispatch(tag, cmd, args, lits)
            except Exception as e:  # noqa: BLE001
                self.send(f"{tag} BAD {e}\r\n")
                continue
            if done == "bye":
                return

    def msgs(self, box=None):
        return STORE.boxes[self.user][box or self.box]

    def dispatch(self, tag, cmd, args, lits):
        ok = lambda t="done": self.send(f"{tag} OK {t}\r\n")  # noqa: E731
        no = lambda t: self.send(f"{tag} NO {t}\r\n")  # noqa: E731
        if cmd == "CAPABILITY":
            self.send("* CAPABILITY IMAP4rev1 MOVE UIDPLUS\r\n")
            return ok()
        if cmd == "LOGIN":
            user, secret = parse_args(args)
            if USERS.get(user) != secret:
                return no("[AUTHENTICATIONFAILED] invalid credentials")
            self.user = user
            STORE.logins += 1
            return ok("logged in")
        if cmd == "LOGOUT":
            self.send("* BYE bye\r\n")
            ok()
            return "bye"
        if not self.user:
            return no("log in first")
        if cmd == "LIST":
            for b in BOXES:
                self.send(f'* LIST ({ATTRS[b]}) "/" {q(b)}\r\n')
            return ok()
        if cmd == "STATUS":
            name = parse_args(args)[0]
            ms = self.msgs(name)
            self.send(f"* STATUS {q(name)} (MESSAGES {len(ms)} UNSEEN {len([m for m in ms if '\\Seen' not in m['flags']])})\r\n")
            return ok()
        if cmd in ("SELECT", "EXAMINE"):
            name = parse_args(args)[0]
            if name not in BOXES:
                return no("no such mailbox")
            self.box = name
            self.send(f"* {len(self.msgs())} EXISTS\r\n")
            return ok("selected")
        if cmd == "UID":
            sub, _, rest = args.partition(" ")
            return getattr(self, "uid_" + sub.lower())(tag, rest, lits, ok, no)
        if cmd == "APPEND":
            words = parse_args(args.replace("\x00LIT0\x00", "").strip())
            name = words[0]
            STORE.uid += 1
            self.msgs(name).append({"uid": STORE.uid, "raw": lits[0], "flags": {"\\Seen"}, "date": "19-Oct-2026 12:00:00 +0000"})
            return ok("appended")
        return no("unknown command " + cmd)

    def uid_search(self, tag, rest, lits, ok, no):
        ms = self.msgs()
        m = re.search(r"TEXT \x00LIT0\x00", rest)
        if m:
            needle = lits[0].decode().lower()
            ms = [x for x in ms if needle in searchable(x["raw"])]
        self.send("* SEARCH " + " ".join(str(x["uid"]) for x in ms) + "\r\n")
        return ok()

    def uid_fetch(self, tag, rest, lits, ok, no):
        set_, _, items = rest.partition(" ")
        want = {int(u) for u in set_.split(",")}
        for seq, msg in enumerate(self.msgs(), 1):
            if msg["uid"] not in want:
                continue
            parts = [f"UID {msg['uid']}", "FLAGS (" + " ".join(sorted(msg["flags"])) + ")", f'INTERNALDATE "{msg["date"]}"']
            tail = b""
            if "BODYSTRUCTURE" in items:
                m = message_from_bytes(msg["raw"], policy=policy.default)
                has = any(p.get_content_disposition() == "attachment" for p in m.walk())
                parts.append('BODYSTRUCTURE (("text" "plain" NIL NIL NIL "7bit" 5 1) ("application" "pdf" NIL NIL NIL "base64" 9 NIL ("attachment" ("filename" "x")))  "mixed")' if has else 'BODYSTRUCTURE ("text" "plain" NIL NIL NIL "7bit" 5 1)')
            if "HEADER.FIELDS" in items:
                m = message_from_bytes(msg["raw"])
                head = "".join(f"{k}: {m.get(k, '')}\r\n" for k in ("From", "Subject")) + "\r\n"
                parts.append("BODY[HEADER.FIELDS (FROM SUBJECT)] {%d}\r\n%s" % (len(head.encode()), head))
            if "BODY.PEEK[]" in items:
                parts.append("BODY[] {%d}\r\n" % len(msg["raw"]))
                self.send(f"* {seq} FETCH (" + " ".join(parts[:-1]) + " " + parts[-1])
                self.send(msg["raw"] + b")\r\n")
                continue
            self.send(f"* {seq} FETCH (" + " ".join(parts) + ")\r\n")
        return ok()

    def uid_store(self, tag, rest, lits, ok, no):
        uid, op, flags = rest.split(" ", 2)
        for msg in self.msgs():
            if msg["uid"] == int(uid):
                names = set(flags.strip("()").split())
                if op.startswith("+"):
                    msg["flags"] |= names
                else:
                    msg["flags"] -= names
        return ok()

    def uid_move(self, tag, rest, lits, ok, no):
        uid, name = parse_args(rest)
        src = self.msgs()
        for msg in list(src):
            if msg["uid"] == int(uid):
                src.remove(msg)
                self.msgs(name).append(msg)
        return ok()

    def uid_copy(self, tag, rest, lits, ok, no):
        uid, name = parse_args(rest)
        for msg in self.msgs():
            if msg["uid"] == int(uid):
                self.msgs(name).append(dict(msg, flags=set(msg["flags"])))
        return ok()


class Smtp(socketserver.StreamRequestHandler):
    def send(self, t):
        self.wfile.write(t.encode())
        self.wfile.flush()

    def handle(self):
        self.send("220 fake smtp\r\n")
        user, sender, rcpts = None, None, []
        while True:
            line = self.rfile.readline()
            if not line:
                return
            line = line.decode().rstrip("\r\n")
            up = line.upper()
            if up.startswith("EHLO") or up.startswith("HELO"):
                self.send("250-fake\r\n250 AUTH PLAIN LOGIN\r\n")
            elif up.startswith("AUTH PLAIN"):
                parts = base64.b64decode(line.split(" ", 2)[2]).split(b"\0")
                name, pw = parts[1].decode(), parts[2].decode()
                if USERS.get(name.split("@")[0]) != pw:
                    self.send("535 bad login\r\n")
                else:
                    user = name.split("@")[0]
                    self.send("235 ok\r\n")
            elif up.startswith("MAIL FROM"):
                sender = re.search(r"<(.*?)>", line).group(1)
                rcpts = []
                self.send("250 ok\r\n")
            elif up.startswith("RCPT TO"):
                rcpts.append(re.search(r"<(.*?)>", line).group(1))
                self.send("250 ok\r\n")
            elif up == "DATA":
                self.send("354 go\r\n")
                data = b""
                while True:
                    l = self.rfile.readline()
                    if l == b".\r\n":
                        break
                    data += l[1:] if l.startswith(b"..") else l
                STORE.submitted.append({"user": user, "from": sender, "rcpts": rcpts, "raw": data, "message": message_from_bytes(data, policy=policy.default)})
                self.send("250 queued\r\n")
            elif up == "QUIT":
                self.send("221 bye\r\n")
                return
            else:
                self.send("250 ok\r\n")


class Quiet(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def start(imap_port=19143, smtp_port=19025):
    servers = [Quiet(("127.0.0.1", imap_port), Handler), Quiet(("127.0.0.1", smtp_port), Smtp)]
    for s in servers:
        threading.Thread(target=s.serve_forever, daemon=True).start()
    return servers
