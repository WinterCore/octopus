import { html, LitElement } from "lit";
import { customElement, state } from "lit/decorators.js";
import "./components/stream-picker";
import "./components/stream-player";
import "./components/admin/admin-login";
import "./components/admin/admin-dashboard";
import { listStreams, UnauthorizedError } from "./lib/admin-api";
import { navigate, parseRoute, type Route } from "./lib/router";

type AdminAuthState = "unknown" | "logged-out" | "logged-in";

@customElement("octopus-app")
export class Octopus extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @state()
  private route: Route = parseRoute(window.location.pathname);

  @state()
  private adminAuth: AdminAuthState = "unknown";

  private popstateListener = () => {
    this.route = parseRoute(window.location.pathname);
    if (this.route.kind === "admin") {
      void this.checkAdminAuth();
    }
  };

  private clickListener = (event: MouseEvent) => {
    // Intercept clicks on internal anchor links so they use pushState instead
    // of a full page navigation.
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;

    const anchor = (event.target as HTMLElement | null)?.closest?.("a");
    if (!anchor) return;
    if (anchor.target && anchor.target !== "_self") return;
    if (anchor.hasAttribute("download") || anchor.getAttribute("rel") === "external") return;

    const href = anchor.getAttribute("href");
    if (!href || href.startsWith("http://") || href.startsWith("https://") || href.startsWith("//")) return;
    if (href.startsWith("#") || href.startsWith("mailto:") || href.startsWith("tel:")) return;

    event.preventDefault();
    const url = new URL(href, window.location.origin);
    if (url.pathname !== window.location.pathname) {
      window.history.pushState({}, "", url.pathname + url.search);
    }
    this.route = parseRoute(url.pathname);
    if (this.route.kind === "admin") {
      void this.checkAdminAuth();
    }
  };

  connectedCallback(): void {
    super.connectedCallback();
    window.addEventListener("popstate", this.popstateListener);
    document.addEventListener("click", this.clickListener);
    if (this.route.kind === "admin") {
      void this.checkAdminAuth();
    }
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    window.removeEventListener("popstate", this.popstateListener);
    document.removeEventListener("click", this.clickListener);
  }

  private async checkAdminAuth(): Promise<void> {
    try {
      await listStreams();
      this.adminAuth = "logged-in";
    } catch (e) {
      if (e instanceof UnauthorizedError) {
        this.adminAuth = "logged-out";
      } else {
        // Treat network errors as logged-out so the user sees the login form rather than a hang.
        this.adminAuth = "logged-out";
      }
    }
  }

  private handleLoggedIn = () => {
    this.adminAuth = "logged-in";
  };

  private handleLoggedOut = () => {
    this.adminAuth = "logged-out";
    navigate({ kind: "picker" });
  };

  private handleUnauthorized = () => {
    this.adminAuth = "logged-out";
  };

  render() {
    switch (this.route.kind) {
      case "picker":
        return html`<stream-picker></stream-picker>`;
      case "listen":
        return html`<stream-player .streamId=${this.route.streamId}></stream-player>`;
      case "admin":
        if (this.adminAuth === "unknown") {
          return html`
            <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1 flex items-center justify-center">
              <div class="w-full max-w-sm h-24 bg-white/5 rounded-xl animate-pulse"></div>
            </main>
          `;
        }
        if (this.adminAuth === "logged-out") {
          return html`<admin-login @admin-logged-in=${this.handleLoggedIn}></admin-login>`;
        }
        return html`
          <admin-dashboard
            @admin-logged-out=${this.handleLoggedOut}
            @admin-unauthorized=${this.handleUnauthorized}
          ></admin-dashboard>
        `;
    }
  }
}
