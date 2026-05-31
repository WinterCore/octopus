export type Route =
  | { kind: "picker" }
  | { kind: "listen"; streamId: string }
  | { kind: "admin" };

export function parseRoute(pathname: string): Route {
  if (pathname === "" || pathname === "/") return { kind: "picker" };

  if (pathname === "/admin" || pathname.startsWith("/admin/")) return { kind: "admin" };

  const listenMatch = pathname.match(/^\/listen\/(.+)$/);
  if (listenMatch) {
    return { kind: "listen", streamId: decodeURIComponent(listenMatch[1]) };
  }

  return { kind: "picker" };
}

export function routeToPath(route: Route): string {
  switch (route.kind) {
    case "picker":
      return "/";
    case "listen":
      return `/listen/${encodeURIComponent(route.streamId)}`;
    case "admin":
      return "/admin";
  }
}

export function navigate(route: Route): void {
  const path = routeToPath(route);
  if (window.location.pathname !== path) {
    window.history.pushState({}, "", path);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }
}
