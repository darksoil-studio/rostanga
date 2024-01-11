import { event } from "@tauri-apps/api";
import {
  Writable,
  derived,
  subscribe,
  writable,
} from "@holochain-open-dev/stores";
import { customElement, state } from "lit/decorators.js";
import { LitElement, html, css } from "lit";
import "@shoelace-style/shoelace/dist/components/progress-bar/progress-bar.js";
import "@shoelace-style/shoelace/dist/components/button/button.js";
import "@shoelace-style/shoelace/dist/components/spinner/spinner.js";
import { styleMap } from "lit/directives/style-map.js";
import { msg } from "@lit/localize";
import { invoke } from "@tauri-apps/api/core";

const STATUS = [
  "start",
  // "lair-keystore-launched",
  "holochain-ready",
  "notifications-setup-complete",
  "gather-setup-complete",
];

const status: Writable<string> = writable(STATUS[0]);
const progress = derived(status, (status) => {
  const statusIndex = STATUS.findIndex((s) => s === status);
  return Math.floor((statusIndex + 1) / STATUS.length) * 100;
});

event.listen("setup-progress", (e) => {
  status.set(e.payload as string);
});

event.listen("holochain-ready", () => {
  status.set("holochain-ready");
});

@customElement("splash-screen")
export class SplashScreen extends LitElement {
  @state()
  currentPage: number = 0;

  pages() {
    return [
      () => this.renderWelcome(),
      () => this.renderContext(),
      () => this.renderStatus(),
      () => this.renderGather1(),
      () => this.renderGather2(),
      () => this.renderFeedback(),
      () => this.renderThanks(),
    ];
  }

  renderWelcome() {
    return html`<div class="column" style="gap: 16px">
      <h2>${msg("Welcome to the Röstånga app!")}</h2>
      <span
        >${msg("Everything that is relevant to Röstånga, in one place.")}</span
      >
    </div>`;
  }

  renderContext() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "Connecting with each other is hard when everyone is using a different app."
        )}</span
      >
      <span
        >${msg(
          "What if we could tie our own technical infrastructure to our village?"
        )}</span
      >
    </div>`;
  }

  renderStatus() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "At darksoil studio, we want to make this dream a reality."
        )}</span
      >
      <span
        >${msg(
          "This app is the first step in that direction, an experiment."
        )}</span
      >
      <span
        >${msg(
          "For now, it only includes Gather, an app to organize events in a collaborative way."
        )}</span
      >
    </div>`;
  }

  renderGather1() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "In gather, you'll be able to propose events, and invite others around you to join them."
        )}</span
      >
      <span
        >${msg(
          "Imagine you want to play a football match. It's not worth it to meet unless we have at least 10 players to play! Oh and if no one has a ball, then we can't play either!"
        )}</span
      >
    </div>`;
  }

  renderGather2() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "In gather, you can set a minimum number of participants or required needs for your events."
        )}</span
      >
      <span
        >${msg(
          "If the proposal is interesting to other participants, they will start contributing to that proposal. If those needs are not met, then the proposal fails and the event never happens."
        )}</span
      >
      <span
        >${msg(
          "But if all the needs are met, then the proposal succeeds and we get to have a great time with each other doing what we love doing!"
        )}</span
      >
    </div>`;
  }

  renderFeedback() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "Gather is still a prototype, so we want to get as much feedback about it as we can."
        )}</span
      >
      <span
        >${msg(
          "Play with it! What's working? What's not? Tell us about it!"
        )}</span
      >
    </div>`;
  }

  renderThanks() {
    return html`<div class="column" style="gap: 16px">
      <span
        >${msg(
          "We are really excited to start this journey, and we invite you to join us!"
        )}</span
      >
      <span>${msg("Thanks so much for being here and trying this out.")}</span>
    </div>`;
  }

  renderProgress() {
    return html`${subscribe(
      progress,
      (p) => html`<sl-progress-bar .value=${p}></sl-progress-bar>`
    )}`;
  }

  renderCurrentPage() {
    return this.pages()[this.currentPage]();
  }

  renderActions() {
    const lastPage = this.currentPage === this.pages().length - 1;
    return html`
      <div class="row" style="gap: 8px">
        <sl-button
          .style=${styleMap({
            display: this.currentPage === 0 ? "none" : "auto",
          })}
          @click=${() => (this.currentPage -= 1)}
        >
          ${msg("Previous")}
        </sl-button>
        ${subscribe(
          progress,
          (p) => html`
            <sl-button
              .disabled=${lastPage && p !== 100}
              @click=${() =>
                lastPage
                  ? invoke("open_app", {
                      appId: "gather",
                    })
                  : (this.currentPage += 1)}
            >
              ${lastPage ? msg("Launch App") : msg("Next")}
            </sl-button>
          `
        )}
      </div>
    `;
  }

  renderSplashScreen() {
    return html`<div class="column" style="gap: 16px">
      ${this.renderCurrentPage()} ${this.renderActions()}
      ${this.renderProgress()}
    </div>`;
  }

  renderLoading() {
    return html`<div
      class="column"
      style="align-items: center; justify-content: center"
    >
      <sl-spinner style="font-size: 2rem"></sl-spinner>
    </div>`;
  }

  render() {
    return html`${subscribe(status, (status) =>
      status === STATUS[0] ? this.renderLoading() : this.renderSplashScreen()
    )}`;
  }

  static styles = css`
    .row {
      display: flex;
      flex-direction: row;
    }
    .column {
      display: flex;
      flex-direction: column;
    }
  `;
}
