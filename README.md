# Octopus

A live radio streaming server that broadcasts **multiple concurrent streams** from a single process. Each stream continuously plays `.opus` files from its own playlist directory and streams them in real time over HTTP. Any number of clients can connect to any stream and hear it live. A WebSocket connection per stream provides track metadata and pushes updates when tracks change. An admin web UI (password-gated) lets you skip tracks, pause/resume, and rename streams.

## How it works

Streams are defined statically in a TOML config file loaded at startup. For each stream, the backend spawns its own player task that decodes Opus audio, re-encodes it to normalize frame sizes, and broadcasts frames to all HTTP clients listening on that stream. A 3-second buffer per stream lets new clients join mid-stream without waiting for the next frame.

The frontend has three views:
- **Stream picker** at `/` — lists all configured streams, click one to listen.
- **Player** at `/listen/<id>` — plays the chosen stream with artwork, title, and a circular progress ring. Shows a `PAUSED` badge when the admin has paused it.
- **Admin** at `/admin` — password login, then a dashboard to skip / pause / resume / rename each stream.

## Configuration

### Streams (TOML)

Streams live in a TOML file passed via `--config`:

```toml
default_stream = "main"

[[stream]]
id = "main"
name = "Main Station"
playlist = "/path/to/main/playlist"

[[stream]]
id = "chill"
name = "Chill Vibes"
playlist = "/path/to/chill/playlist"
```

`id` is the URL-stable identifier; `name` is the display name (editable by admin and persisted back to the file). See [`streams.example.toml`](./streams.example.toml). The file must be writable for admin renames to persist.

### Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `HTTP_PORT` | — | HTTP audio + admin API port |
| `WS_PORT` | — | WebSocket metadata port |
| `ADMIN_PASSWORD` | — | Shared password for the admin UI |
| `CONTROL_PIPE` | `./control.fifo` | FIFO that writes route to the **default** stream (fallback / debugging) |

## Running

### Backend

```bash
HTTP_PORT=9000 WS_PORT=9001 ADMIN_PASSWORD=secret \
  cargo run --release -- --config streams.toml
```

Each stream starts playing its playlist immediately. To override the default stream's playlist on the fly:

```bash
echo "/path/to/other/music" > control.fifo
```

### Frontend

A Lit + Vite + TailwindCSS web player.

```bash
cd web
npm install
npm run dev
```

## HTTP API

Public:

- `GET /streams` — JSON list of streams (`id`, `name`, `paused`, current `title`/`author`)
- `GET /streams/{id}/audio` — Ogg Opus audio stream for that stream
- `GET /streams/{id}/playlist-image` — Serves `playlist.jpg` from that stream's playlist directory

Admin (cookie session from `POST /admin/login`):

- `POST /admin/login` — body `{ "password": "…" }` → sets the `octopus_admin` cookie
- `POST /admin/logout` — clears the session
- `GET /admin/streams` — same as `/streams` plus the playlist path
- `POST /admin/streams/{id}/skip` — skip the current track
- `POST /admin/streams/{id}/pause` — pause the stream (packet production halts; connected listeners stall until resume)
- `POST /admin/streams/{id}/resume` — resume
- `PATCH /admin/streams/{id}` — body `{ "name": "…" }` → rename, persisted to the TOML

CORS responses echo the request `Origin` and set `Access-Control-Allow-Credentials: true` so the admin UI can send the session cookie from a different origin.

## WebSocket

`GET ws://…/streams/{id}` upgrades to a WebSocket for that stream. Send `"metadata"` to receive current track info. The server also pushes metadata automatically when the track changes. Payload includes `title`, `author`, `image`, `paused`, `stream_id`, `stream_name`, and timing fields used by the frontend to drive the progress ring.

## Playlist image

Place a `playlist.jpg` file in a stream's playlist directory and it will be served at `/streams/{id}/playlist-image`. The frontend hashes responses and only swaps the on-screen artwork when the bytes actually change, so it doesn't flicker between tracks on the same playlist.

## Deployment

A systemd service template is at `deploy/octopus.service`. Replace `__DEPLOY_PATH__` with the binary location and install with:

```bash
cp deploy/octopus.service ~/.config/systemd/user/octopus.service
systemctl --user enable --now octopus
```

The service must be configured to pass `--config <path>` and set `ADMIN_PASSWORD`. The frontend uses pathname routing, so the static host (nginx, etc.) needs an SPA fallback to `index.html` for unknown paths.
