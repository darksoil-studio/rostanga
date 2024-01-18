import { event } from "@tauri-apps/api";
import {
  AsyncStatus,
  Readable,
  Writable,
  derived,
  subscribe,
  writable,
} from "@holochain-open-dev/stores";
import { customElement, state } from "lit/decorators.js";
import { LitElement, html, css } from "lit";
import "@holochain-open-dev/elements/dist/elements/display-error.js";
import "@shoelace-style/shoelace/dist/components/progress-bar/progress-bar.js";
import "@shoelace-style/shoelace/dist/components/button/button.js";
import "@shoelace-style/shoelace/dist/components/spinner/spinner.js";
import { styleMap } from "lit/directives/style-map.js";
import { msg } from "@lit/localize";
import { invoke } from "@tauri-apps/api/core";
import { renderAsyncStatus } from "@holochain-open-dev/elements";
import { AppInfo } from "@holochain/client";

//@ts-ignore
import rostangaSplash from "../rostanga-splash.jpg";

import { setLocale } from "./locales";

const setupError: Writable<string | undefined> = writable(undefined);
event.listen("setup-error", (e) => {
  setupError.set(e.payload as string);
});

const INITIAL_APPS = [
  "gather",
  "notifications_fcm_recipient",
  "notifications_provider_fcm",
];

const initialAppsSetup = writable(INITIAL_APPS.map(() => false));
function appHasBeenSetUp(appId: string) {
  initialAppsSetup.update((apps) => {
    const index = INITIAL_APPS.findIndex((a) => a === appId);
    apps[index] = true;

    return apps;
  });
}
event.listen("app-installed", (e) => {
  appHasBeenSetUp(e.payload as string);
});

async function openGather() {
  invoke("launch_gather");
}

function afterHolochainReady() {
  invoke("plugin:holochain|list_apps").then((apps) => {
    console.log(apps);
    for (const app of apps as Array<AppInfo>) {
      appHasBeenSetUp(app.installed_app_id);
    }
  });
}

const holochainReady = writable(false);
invoke("plugin:holochain|is_holochain_ready")
  .then((v) => {
    holochainReady.set(v as boolean);
    if (v as boolean) {
      afterHolochainReady();
    }
  })
  .catch(() => holochainReady.set(false));
event.listen("holochain-ready", () => {
  holochainReady.set(true);
  afterHolochainReady();
});

const progress = derived(
  [holochainReady, initialAppsSetup],
  ([hcReady, readys]) => {
    const setups = [hcReady, ...readys];
    const setupsDone = setups.filter((i) => i);

    return Math.floor((100 * setupsDone.length) / setups.length);
  }
);
const status: Readable<AsyncStatus<number>> = derived(
  [setupError, holochainReady, progress],
  ([setupError, _holochainReady, progress]) => {
    if (setupError)
      return {
        status: "error",
        error: setupError,
      } as AsyncStatus<number>;
    return {
      status: "complete",
      value: progress,
    } as AsyncStatus<number>;
  }
);

@customElement("splash-screen")
export class SplashScreen extends LitElement {
  @state()
  currentPage: number = 0;

  @state()
  isAndroid = false;

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

  async firstUpdated() {
    setLocale();
    this.isAndroid = await invoke("is_android");
  }

  renderWelcome() {
    return html`<div class="page">
      <h2>${msg("Welcome to the Röstånga app!")}</h2>
      <span
        >${msg("Everything that is relevant to Röstånga, in one place.")}</span
      >
    </div>`;
  }

  renderContext() {
    return html`<div class="page">
      <span
        >${msg(
          "Connecting with each other is hard when everyone is using a different app."
        )}</span
      >
      <span
        >${msg(
          "What if we could tie our own technical infrastructure to our village, and bring all that we need in a single place?"
        )}</span
      >
    </div>`;
  }

  renderStatus() {
    return html`<div class="page">
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
    return html`<div class="page">
      <span
        >${msg(
          "In gather, you'll be able to propose events, and invite others around you to join them."
        )}</span
      >
      <span
        >${msg(
          "Imagine you want to play a football match. It's not worth it to meet unless we have at least 10 players to play! Oh, and if no one has a ball, then we can't play either!"
        )}</span
      >
    </div>`;
  }

  renderGather2() {
    return html`<div class="page">
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
    return html`<div class="page">
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
    return html`<div class="page">
      <span
        >${msg(
          "We are really excited to start this journey, and we invite you to join us!"
        )}</span
      >
      <span>${msg("Thanks so much for being here and trying this out.")}</span>
      ${this.isAndroid
        ? html`
            <span
              >${msg(
                "To start using the app, wait until the setup is completed, then close it and start it again."
              )}</span
            >
          `
        : html``}
    </div>`;
  }

  renderProgress() {
    return html`${subscribe(
      progress,
      (p) => html`<sl-progress-bar .value=${p}></sl-progress-bar>`
      // (_p) => html`<sl-progress-bar indeterminate></sl-progress-bar>`
    )}`;
  }

  renderCurrentPage() {
    return this.pages()[this.currentPage]();
  }

  renderActions() {
    const lastPage = this.currentPage === this.pages().length - 1;
    return html`
      <div class="row" style="gap: 8px;">
        <sl-button
          style=${styleMap({
            flex: 1,
            opacity: this.currentPage === 0 ? "0" : "1",
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
              style="flex: 1"
              .variant=${lastPage ? "primary" : "default"}
              @click=${() =>
                lastPage ? openGather() : (this.currentPage += 1)}
            >
              ${lastPage
                ? this.isAndroid
                  ? msg("Close App")
                  : msg("Launch App")
                : msg("Next")}
            </sl-button>
          `
        )}
      </div>
    `;
  }

  renderSplashScreen() {
    return html` <div class="column">
      <img
        src="${rostangaSplash}"
        style="height: 300px; width: 100%; object-fit: cover"
      />
      <div class="column" style="flex: 1; gap: 16px; margin: 16px">
        <div style="flex: 1">${this.renderCurrentPage()}</div>
        ${this.renderActions()} ${this.renderProgress()}
      </div>
    </div>`;
  }

  renderLoading() {
    return html`<div
      class="column"
      style="flex: 1; align-items: center; justify-content: center"
    >
      <sl-spinner style="font-size: 2rem"></sl-spinner>
    </div>`;
  }

  render() {
    return html`${subscribe(
      status,
      renderAsyncStatus({
        pending: () => this.renderLoading(),
        error: (e) =>
          html`<display-error
            .headline=${msg("Sorry... There was an error launching the app.")}
            .error=${e}
          ></display-error>`,
        complete: () => this.renderSplashScreen(),
      })
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
    :host {
      display: flex;
      flex: 1;
    }
    .page {
      flex: 1;
      gap: 16px;
      display: flex;
      flex-direction: column;
    }

    span {
      text-align: justify;
    }
    h2 {
    }
  `;
}
