export const API_BASE_URL: string =
  import.meta.env.VITE_API_BASE_URL || "http://localhost:3000";

export const WS_BASE_URL: string =
  import.meta.env.VITE_WS_URL || "ws://localhost:3001";

export function streamsListUrl(): string {
  return `${API_BASE_URL}/streams`;
}

export function audioUrl(streamId: string): string {
  return `${API_BASE_URL}/streams/${encodeURIComponent(streamId)}/audio`;
}

export function playlistImageUrl(streamId: string, cacheBuster?: string): string {
  const base = `${API_BASE_URL}/streams/${encodeURIComponent(streamId)}/playlist-image`;
  return cacheBuster ? `${base}?v=${cacheBuster}` : base;
}

export function wsUrl(streamId: string): string {
  return `${WS_BASE_URL}/streams/${encodeURIComponent(streamId)}`;
}

export function adminUrl(path: string): string {
  return `${API_BASE_URL}/admin${path}`;
}
