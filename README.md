# Octopus

A live radio streaming server. The backend continuously plays `.opus` files from a playlist directory and streams them in real time over HTTP. Any number of clients can connect and hear the same live stream. A WebSocket connection provides track metadata and pushes updates when tracks change.

## How it works

The backend decodes Opus audio, re-encodes it to normalize frame sizes, and broadcasts frames to all connected HTTP clients. A 3-second buffer is maintained so new clients can join mid-stream without waiting for the next frame.

Playlists are controlled by writing a directory path to a named FIFO pipe. The backend scans the directory for `.opus` files, sorts them alphabetically, and loops through them continuously. Sending a new path switches playlists immediately.

## Running

### Backend

```bash
HTTP_PORT=9000 WS_PORT=9001 cargo run --release

# Start a playlist
echo "/path/to/music" > control.fifo
```

### Frontend

A SvelteKit + TailwindCSS web player. Shows track title, artist, album art, and a circular SVG progress ring. Connects to the backend over HTTP for audio and WebSocket for metadata.

```bash
cd webplayer
pnpm install
pnpm dev
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `HTTP_PORT` | — | HTTP audio stream port |
| `WS_PORT` | — | WebSocket metadata port |
| `CONTROL_PIPE` | `./control.fifo` | Path to the FIFO control pipe |

## HTTP API

- `GET /` — Ogg Opus audio stream
- `GET /playlist-image` — Serves `playlist.jpg` from the active playlist directory

## WebSocket

Send `"metadata"` to receive current track info. The server also pushes metadata automatically when the track changes.

## Playlist image

Place a `playlist.jpg` file in the playlist directory and it will be served at `/playlist-image`.

## Deployment

A systemd service template is at `deploy/octopus.service`. Replace `__DEPLOY_PATH__` with the binary location and install with:

```bash
cp deploy/octopus.service ~/.config/systemd/user/octopus.service
systemctl --user enable --now octopus
```
