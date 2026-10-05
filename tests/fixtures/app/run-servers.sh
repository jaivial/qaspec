#!/usr/bin/env bash
# Starts the fixture app (18731) and its admin app (18732) in the background.
cd "$(dirname "$0")"
python3 server.py 18731 >/tmp/qaspec-fixture-app.log 2>&1 &
echo $! > /tmp/qaspec-fixture-app.pid
sleep 0.3
python3 server.py 18732 admin >/tmp/qaspec-fixture-admin.log 2>&1 &
echo $! > /tmp/qaspec-fixture-admin.pid
sleep 0.5
curl -sf http://127.0.0.1:18731/health >/dev/null && curl -sf http://127.0.0.1:18732/health >/dev/null && echo "fixture up"
