# Gemini Code Guide: Octopus

This document provides a comprehensive overview of the Octopus project, its architecture, and development conventions.

## Project Overview

Octopus is a web-based radio application designed to stream Opus audio files over HTTP. The project is divided into a backend and two frontend applications.

### Backend

The backend is a Rust application built with Tokio and Hyper. It serves two main purposes:

*   **HTTP Server:** Streams Opus audio files to the frontend.
*   **WebSocket Server:** Provides real-time communication with the frontend for features like playback control and metadata updates.
*   **CLI:** A command-line interface for managing the playlist and playback.

The backend is responsible for reading a directory of `.opus` files, managing the playlist, and streaming the audio data to the connected clients.

### Frontend

There are two frontend applications in this project:

1.  **`web`:** A simple frontend built with Lit, TypeScript, and Vite. It appears to be a lightweight client for the radio stream.
2.  **`webplayer`:** A more feature-rich frontend built with SvelteKit, TypeScript, and Vite. This application provides a user interface for playback control, displays track information, and will eventually handle metadata fetching.

## Building and Running

### Backend

To run the backend, you need to have Rust and Cargo installed.

1.  **Navigate to the project root:**
    ```bash
    cd /home/winter/src/octopus
    ```
2.  **Set the required environment variables:**
    ```bash
    export HTTP_PORT=8080
    export WS_PORT=8081
    ```
3.  **Run the backend:**
    ```bash
    cargo run
    ```
4.  **Provide a playlist directory:**
    The backend will prompt you to enter a path to a directory containing `.opus` files.

### Frontend (`web`)

To run the `web` frontend:

1.  **Navigate to the `web` directory:**
    ```bash
    cd /home/winter/src/octopus/web
    ```
2.  **Install dependencies:**
    ```bash
    npm install
    ```
3.  **Start the development server:**
    ```bash
    npm run dev
    ```

### Frontend (`webplayer`)

To run the `webplayer` frontend:

1.  **Navigate to the `webplayer` directory:**
    ```bash
    cd /home/winter/src/octopus/webplayer
    ```
2.  **Install dependencies:**
    ```bash
    npm install
    ```
3.  **Start the development server:**
    ```bash
    npm run dev
    ```

## Development Conventions

### Backend

*   The backend code is written in Rust and follows the standard Rust conventions.
*   The code is organized into modules, each with a specific responsibility (e.g., `http_server`, `ws_server`, `opus_player`).
*   Asynchronous programming with Tokio is used extensively.

### Frontend

*   Both frontends are written in TypeScript and use Vite for building and development.
*   The `web` frontend uses the Lit library for creating web components.
*   The `webplayer` frontend uses the SvelteKit framework.
*   Both frontends use Tailwind CSS for styling.
