import { html, LitElement } from "lit";
import { customElement, state } from "lit/decorators.js";
import { login } from "../../lib/admin-api";

@customElement("admin-login")
export class AdminLogin extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @state()
  private password: string = "";

  @state()
  private error: string | null = null;

  @state()
  private submitting: boolean = false;

  connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "flex-1");
  }

  private async handleSubmit(e: Event) {
    e.preventDefault();
    this.error = null;
    this.submitting = true;
    try {
      await login(this.password);
      this.dispatchEvent(new CustomEvent("admin-logged-in", { bubbles: true, composed: true }));
    } catch (err) {
      this.error = err instanceof Error ? err.message : "Login failed";
    } finally {
      this.submitting = false;
    }
  }

  render() {
    return html`
      <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1 flex items-center justify-center">
        <form @submit=${this.handleSubmit} class="w-full max-w-sm bg-white/5 border border-white/10 rounded-xl p-6">
          <h1 class="text-white text-xl mb-4">Admin login</h1>
          <label class="block text-white/70 text-sm mb-1" for="admin-password">Password</label>
          <input
            id="admin-password"
            type="password"
            autocomplete="current-password"
            .value=${this.password}
            @input=${(e: Event) => (this.password = (e.target as HTMLInputElement).value)}
            class="w-full bg-black/30 text-white border border-white/10 rounded-md px-3 py-2 focus:outline-none focus:border-white/30"
            ?disabled=${this.submitting}
          />
          ${this.error ? html`<p class="mt-2 text-red-300 text-sm">${this.error}</p>` : ""}
          <button
            type="submit"
            ?disabled=${this.submitting || this.password.length === 0}
            class="mt-4 w-full bg-white/15 hover:bg-white/25 disabled:opacity-50 text-white rounded-md py-2 transition-colors"
          >
            ${this.submitting ? "Signing in…" : "Sign in"}
          </button>
          <div class="mt-4 text-center">
            <a href="/" class="text-white/40 hover:text-white/70 text-xs underline">Back to streams</a>
          </div>
        </form>
      </main>
    `;
  }
}
