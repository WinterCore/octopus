# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Octopus is a live radio streaming application that streams Ogg Opus audio files over HTTP. The project consists of a Rust backend that manages audio playback and streaming, and a TypeScript/Lit frontend web player.

## Architecture

### Backend (Rust/Tokio)

The backend runs three concurrent tasks:
1. **HTTP Server** (port from `HTTP_PORT` env): Streams Ogg Opus audio via HTTP streaming
2. **WebSocket Server** (port from `WS_PORT` env): Sends metadata (title, artist, duration, timestamps)
3. **Player Input Handler**: Reads playlist directory paths from stdin to start playback

**Core Components:**

- **`opus_player.rs`**: Central player managing playback state
  - Maintains a 3-second PCM buffer for headstart (clients can "catch up" to live stream)
  - Decodes Opus packets, re-encodes them, and distributes to HTTP listeners via mpsc channels
  - Tracks granule position for precise timing synchronization
  - Uses `oeggs.rs` module to extract Ogg Opus metadata (title, artist, etc.)
  - Each file has an ID; changing files interrupts current playback

- **`http_server.rs`**: Streams audio via chunked HTTP response
  - Sends Opus header and comment packets on connection
  - Sends headstart buffer (last 3 seconds) for smooth playback start
  - Registers as listener to receive live audio frames
  - Uses `OggStream` wrapper to encode raw Opus data into Ogg pages

- **`ws_server.rs`**: WebSocket server for metadata
  - Clients send "metadata" message to request current track info
  - Returns JSON with title, author, active_file_start_time_ms, active_file_duration_ms, image

- **`oeggs.rs`**: Custom Ogg parser and metadata extractor
  - `OggParser`: Manual Ogg page parser (doesn't use external libs for parsing)
  - `get_opus_comments()`: Extracts OpusTags from second page (handles multi-page comment headers)
  - Returns `OpusComments` struct with vendor string and HashMap of metadata

- **`socket_manager.rs`**: Legacy TCP socket actor (currently unused)

**Playback Flow:**
1. User enters directory path via stdin
2. Backend scans for `.opus` files, sorts alphabetically
3. Loops through playlist, playing each file sequentially
4. For each file:
   - Extracts metadata using `get_opus_comments()`
   - Decodes Opus frames to PCM
   - Re-encodes PCM back to Opus (normalizes frame sizes)
   - Maintains 3-second sliding window buffer
   - Broadcasts frames to all HTTP listeners
   - Sleep/throttle to maintain real-time streaming speed

### Frontend (TypeScript/Lit/Vite)

Built with Lit web components and TailwindCSS.

**Core Components:**

- **`app.ts`**: Main application component (`<octopus-app>`)
  - Manages WebSocket connection for metadata
  - Uses `PlaybackController` (reactive controller) for audio playback
  - Displays track title, artist, and progress

- **`controllers/playback-controller.ts`**: Reactive controller for audio streaming
  - Fetches HTTP audio stream from backend
  - Uses `OggOpusDecoderWebWorker` from `ogg-opus-decoder` package
  - Parses Ogg pages to extract granule position for timestamp tracking
  - Manages AudioContext and schedules audio buffers with 100ms lookahead
  - Exposes `isPlaying` and `currentTimeMs` to host component

- **`components/player-progress.ts`**: SVG circular progress indicator

- **`lib/ws-manager.ts`**: WebSocket connection manager

**Audio Playback Flow:**
1. User clicks play button
2. `PlaybackController.start()` fetches HTTP stream from backend
3. Decoder worker decodes Ogg Opus chunks to PCM
4. PCM data scheduled to AudioContext with buffer lookahead
5. Granule positions parsed from Ogg pages for current time tracking
6. Progress updates trigger host component re-renders

## Development Commands

### Backend
```bash
# Build backend
cargo build

# Run backend (requires HTTP_PORT and WS_PORT env vars)
HTTP_PORT=3000 WS_PORT=3001 cargo run

# Then enter playlist directory path in stdin to start playback
```

### Frontend
```bash
cd web

# Install dependencies
npm install

# Run dev server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview
```

## Key Technical Details

### Ogg Opus Comment Metadata
- Comments are on the second Ogg page after OpusHead
- **Can span multiple pages** - segments with size 255 indicate continuation
- Format: `FIELDNAME=value` (case-insensitive keys)
- Standard fields: TITLE, ARTIST, ALBUM, DATE, GENRE, COMPOSER, etc.
- The `get_opus_comments()` function in `backend/src/oeggs.rs` handles multi-page parsing

### Timing Synchronization
- Backend tracks granule position (sample count at 48kHz)
- Frontend parses granule from Ogg pages: `(granuleLow + granuleHigh * 0x100000000)`
- Convert to milliseconds: `granule_position / 48000 * 1000`
- Each track's `active_file_start_time_ms` = granule when file started
- Current position = `currentTimeMs - active_file_start_time_ms`

### Buffer Management
- Backend maintains 3-second sliding window buffer
- New HTTP clients receive this buffer as "headstart"
- Prevents new clients from waiting for next frame
- Frontend uses 100ms AudioContext lookahead for smooth playback

## Project TODO (from README)

**Backend:**
- Add proper playlist management TUI
- Add playlist image support
- Add CLI commands for playback control (next, previous, pause, resume)

**Frontend:**
- Add loading UI when buffering/fetching metadata
- Refetch metadata when track ends
