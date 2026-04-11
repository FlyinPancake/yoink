#!/bin/sh
set -eu

PUID="${PUID:-1000}"
PGID="${PGID:-1000}"

# Create or adjust group
group_recreated=false
if ! grep -q '^yoink:' /etc/group; then
    addgroup -g "$PGID" -S yoink
else
    existing_gid="$(awk -F: '$1 == "yoink" { print $3 }' /etc/group)"
    if [ "$existing_gid" != "$PGID" ]; then
        if id yoink >/dev/null 2>&1; then
            deluser yoink
        fi
        delgroup yoink
        addgroup -g "$PGID" -S yoink
        group_recreated=true
    fi
fi

# Create or adjust user
if ! id yoink >/dev/null 2>&1 || [ "$group_recreated" = true ]; then
    adduser -u "$PUID" -S -D -H -h /app -s /bin/sh -G yoink yoink
else
    existing_uid="$(id -u yoink)"
    if [ "$existing_uid" != "$PUID" ]; then
        deluser yoink
        adduser -u "$PUID" -S -D -H -h /app -s /bin/sh -G yoink yoink
    fi
fi

# Ensure ownership of writable directories
chown -R yoink:yoink /app /data /music

# Drop privileges and exec the main process
exec su-exec yoink "$@"
