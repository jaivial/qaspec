"""Tiny demo app for qaspec integration tests: login (cookie session), a todo list,
an /api/todos endpoint, a deliberately broken page and a second 'admin' app on another port.
Usage: python3 server.py <port> [admin]"""
import http.server, json, os, sys, tempfile, urllib.parse, http.cookies

PORT = int(sys.argv[1]); ADMIN = len(sys.argv) > 2 and sys.argv[2] == "admin"
USERS = {"qa@example.com": "s3cret-pass", "viewer@example.com": "v1ewer-pass"}
# Both processes (app and admin) share the todo list through a file.
DB = os.path.join(tempfile.gettempdir(), "qaspec-fixture-todos.json")
if not ADMIN:
    json.dump(["Buy milk"], open(DB, "w"))

def todos():
    try:
        return json.load(open(DB))
    except (OSError, ValueError):
        return []

def add_todo(text):
    t = todos(); t.append(text); json.dump(t, open(DB, "w"))

CSS = """body{margin:0;font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif;color:#1f2328;background:#f6f8fa}
nav{display:flex;gap:20px;align-items:center;padding:0 32px;height:52px;background:#fff;border-bottom:1px solid #d0d7de}
nav b{margin-right:16px}nav a{color:#57606a;text-decoration:none}nav a:hover{color:#1f2328}
main{max-width:640px;margin:32px auto;padding:24px 28px;background:#fff;border:1px solid #d0d7de;border-radius:8px}
h1{font-size:22px;margin:0 0 12px}label{display:block;margin:12px 0 4px;color:#57606a;font-size:13px}
input{display:block;width:100%;box-sizing:border-box;margin-top:4px;padding:8px 10px;font:inherit;border:1px solid #d0d7de;border-radius:6px}
button{margin-top:14px;padding:8px 16px;font:inherit;font-weight:600;color:#fff;background:#1f883d;border:0;border-radius:6px}
ul{padding:0;list-style:none;margin:0 0 8px}li{padding:10px 0;border-bottom:1px solid #eaeef2}
[role=alert]{color:#cf222e}"""

PAGE = """<!doctype html><html><head><title>{title}</title><meta name="viewport" content="width=device-width, initial-scale=1"><style>""" + CSS.replace("{", "{{").replace("}", "}}") + """</style></head><body>
<nav><b>Todo demo</b><a href="/">Home</a> <a href="/todos">Todos</a> <a href="/broken">Broken</a> <a href="/logout">Log out</a></nav>
<main>{body}</main></body></html>"""

LOGIN = """<h1>Sign in</h1><form method="post" action="/login">
<label>Email <input name="email" type="email"></label>
<label>Password <input name="password" type="password"></label>
<button type="submit">Sign in</button></form>{err}"""

TODO_PAGE = """<h1>Todos</h1><ul id="list"></ul>
<label>New todo <input id="new" aria-label="New todo"></label><button id="add">Add</button>
<script>
async function load(){const r=await fetch('/api/todos');const t=await r.json();window.appState={count:t.length};
document.getElementById('list').innerHTML=t.map(x=>'<li>'+x+'</li>').join('');}
document.getElementById('add').onclick=async()=>{const v=document.getElementById('new').value;
await fetch('/api/todos',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({text:v})});
document.getElementById('new').value='';localStorage.setItem('lastTodo',v);load();};
load();
</script>"""

BROKEN = """<h1>Reports</h1><p>Loading report…</p>
<script>fetch('/api/report').then(r=>{if(!r.ok)console.error('report failed: '+r.status)});
setTimeout(()=>{undefinedFunction()},10);</script>"""

class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def user(self):
        c = http.cookies.SimpleCookie(self.headers.get("Cookie", ""))
        return c["sid"].value if "sid" in c and c["sid"].value in USERS else None
    def send(self, code, body, ctype="text/html", headers=()):
        b = body.encode(); self.send_response(code)
        self.send_header("content-type", ctype); self.send_header("content-length", str(len(b)))
        for k, v in headers: self.send_header(k, v)
        self.end_headers(); self.wfile.write(b)
    def page(self, title, body): self.send(200, PAGE.format(title=title, body=body))
    def redirect(self, to, headers=()): self.send(302, "", headers=[("location", to), *headers])
    def do_GET(self):
        p = urllib.parse.urlparse(self.path).path
        if p == "/health": return self.send(200, "ok", "text/plain")
        if ADMIN:
            return self.page("Admin", f"<h1>Admin console</h1><p>Todos in the system: {len(todos())}</p>")
        if p == "/login": return self.page("Sign in", LOGIN.format(err=""))
        if p == "/logout": return self.redirect("/login", [("set-cookie", "sid=; Max-Age=0; Path=/")])
        u = self.user()
        if not u: return self.redirect("/login")
        if p == "/": return self.page("Home", f"<h1>Welcome, {u}</h1><p>You have {len(todos())} todos.</p>")
        if p == "/todos": return self.page("Todos", TODO_PAGE)
        if p == "/broken": return self.page("Reports", BROKEN)
        if p == "/api/todos": return self.send(200, json.dumps(todos()), "application/json")
        if p == "/api/report": return self.send(500, '{"error":"boom"}', "application/json")
        self.send(404, PAGE.format(title="Not found", body="<h1>Page not found</h1>"))
    def do_POST(self):
        p = urllib.parse.urlparse(self.path).path
        n = int(self.headers.get("content-length", 0)); data = self.rfile.read(n).decode()
        if p == "/login":
            f = urllib.parse.parse_qs(data)
            e, pw = f.get("email", [""])[0], f.get("password", [""])[0]
            if USERS.get(e) == pw:
                return self.redirect("/", [("set-cookie", f"sid={e}; Path=/; HttpOnly")])
            return self.page("Sign in", LOGIN.format(err="<p role='alert'>Invalid email or password</p>"))
        if p == "/api/todos" and self.user():
            add_todo(json.loads(data)["text"]); return self.send(201, "{}", "application/json")
        self.send(401, "{}", "application/json")

http.server.ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
