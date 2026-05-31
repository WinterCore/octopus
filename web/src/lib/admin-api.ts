import { adminUrl } from "./env";

export interface AdminStream {
  id: string;
  name: string;
  paused: boolean;
  title?: string;
  author?: string;
  playlist?: string;
}

export class UnauthorizedError extends Error {
  constructor() {
    super("Unauthorized");
    this.name = "UnauthorizedError";
  }
}

async function call(path: string, init?: RequestInit): Promise<Response> {
  const response = await fetch(adminUrl(path), {
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (response.status === 401) {
    throw new UnauthorizedError();
  }
  return response;
}

export async function login(password: string): Promise<void> {
  const response = await fetch(adminUrl("/login"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ password }),
  });
  if (response.status === 401) {
    throw new Error("Invalid password");
  }
  if (!response.ok) {
    throw new Error(`Login failed: ${response.status}`);
  }
}

export async function logout(): Promise<void> {
  await call("/logout", { method: "POST" });
}

export async function listStreams(): Promise<AdminStream[]> {
  const response = await call("/streams");
  if (!response.ok) throw new Error(`Failed to list streams: ${response.status}`);
  return response.json();
}

export async function skip(streamId: string): Promise<void> {
  const response = await call(`/streams/${encodeURIComponent(streamId)}/skip`, { method: "POST" });
  if (!response.ok) throw new Error(`Skip failed: ${response.status}`);
}

export async function pause(streamId: string): Promise<void> {
  const response = await call(`/streams/${encodeURIComponent(streamId)}/pause`, { method: "POST" });
  if (!response.ok) throw new Error(`Pause failed: ${response.status}`);
}

export async function resume(streamId: string): Promise<void> {
  const response = await call(`/streams/${encodeURIComponent(streamId)}/resume`, { method: "POST" });
  if (!response.ok) throw new Error(`Resume failed: ${response.status}`);
}

export async function rename(streamId: string, name: string): Promise<void> {
  const response = await call(`/streams/${encodeURIComponent(streamId)}`, {
    method: "PATCH",
    body: JSON.stringify({ name }),
  });
  if (!response.ok) throw new Error(`Rename failed: ${response.status}`);
}
